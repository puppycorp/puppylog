use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::LogEntry;

// We will store metadata in the first 32 bytes:
// 0..4:  chunk_size as u32 (little-endian)
// 4..12: total_chunks as u64 (little-endian)
// 12..20: head as u64 (little-endian)
// 20..28: tail as u64 (little-endian)
// 28..32: (unused/reserved)

const METADATA_SIZE: u64 = 32;
const DEFAULT_CHUNK_SIZE: usize = 4096; // 4KB

// Represents one chunk of data in memory.
struct Chunk {
    inx: usize,      // which chunk index in the file
    dirty: bool,     // whether it was modified and needs flush
    data: Vec<u8>,   // actual contents of this chunk
}

impl Chunk {
    fn new(inx: usize, size: usize) -> Self {
        Self {
            inx,
            dirty: false,
            data: vec![0u8; size],
        }
    }

    fn write(&mut self, offset: usize, data: &[u8]) {
        self.data[offset..offset + data.len()].copy_from_slice(data);
        self.dirty = true;
    }

    fn read(&self, offset: usize, dst: &mut [u8]) {
        dst.copy_from_slice(&self.data[offset..(offset + dst.len())]);
    }
}

pub struct CircleBuffer {
    file: File,
    chunk_size: usize,
    num_chunks: usize,
    head: usize, // next write position in buffer
    tail: usize, // next read position in buffer
    max_cached_chunks: usize,
    chunks: Vec<Chunk>, // minimal chunk cache

    // Additional field to hold how many bytes we've read but not yet "committed" (acknowledged)
    uncommitted_read: usize,
}

impl CircleBuffer {
    pub fn new<P: AsRef<Path>>(
        path: P,
        total_chunks: usize,
        chunk_size: usize,
        max_cached_chunks: usize,
    ) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        // Ensure the file is large enough for metadata + all chunks.
        let file_size = METADATA_SIZE + (chunk_size as u64) * (total_chunks as u64);
        file.set_len(file_size)?;

        // Try reading existing metadata.
        let (head, tail) = if file.metadata()?.len() >= METADATA_SIZE {
            let mut metadata = [0u8; METADATA_SIZE as usize];
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut metadata)?;

            let _chunk_size_from_file = u32::from_le_bytes(metadata[0..4].try_into().unwrap());
            let _total_chunks_from_file = u64::from_le_bytes(metadata[4..12].try_into().unwrap());
            let head = u64::from_le_bytes(metadata[12..20].try_into().unwrap());
            let tail = u64::from_le_bytes(metadata[20..28].try_into().unwrap());

            (head, tail)
        } else {
            // File is empty or metadata not written yet.
            // Initialize metadata.
            let mut metadata = [0u8; METADATA_SIZE as usize];
            metadata[0..4].copy_from_slice(&(chunk_size as u32).to_le_bytes());
            metadata[4..12].copy_from_slice(&(total_chunks as u64).to_le_bytes());
            // head & tail start at 0.
            metadata[12..20].copy_from_slice(&0u64.to_le_bytes());
            metadata[20..28].copy_from_slice(&0u64.to_le_bytes());

            file.seek(SeekFrom::Start(0))?;
            file.write_all(&metadata)?;

            (0, 0)
        };

        Ok(Self {
            file,
            chunk_size,
            num_chunks: total_chunks,
            head: head as usize,
            tail: tail as usize,
            max_cached_chunks,
            chunks: Vec::with_capacity(max_cached_chunks),
            uncommitted_read: 0,
        })
    }

    fn capacity(&self) -> usize {
        self.num_chunks * self.chunk_size
    }

    fn used(&self) -> usize {
        let capacity = self.capacity();
        if self.head >= self.tail {
            self.head - self.tail
        } else {
            capacity - (self.tail - self.head)
        }
    }

    fn free_space(&self) -> usize {
        self.capacity() - self.used()
    }

    // Fetch a chunk for the given absolute byte offset in the buffer.
    // If it's in the cache, return it. Otherwise, read it from disk.
    fn get_chunk(&mut self, offset: usize) -> &mut Chunk {
        let inx = offset / self.chunk_size;

        // If chunk is already in the cache, return it.
        if let Some(pos) = self.chunks.iter().position(|c| c.inx == inx) {
            return &mut self.chunks[pos];
        }

        // Not in cache; read it in.
        let file_offset = (inx * self.chunk_size) as u64;
        self.file
            .seek(SeekFrom::Start(METADATA_SIZE + file_offset))
            .expect("failed to seek");

        let mut new_chunk = Chunk::new(inx, self.chunk_size);
        self.file
            .read_exact(&mut new_chunk.data)
            .expect("failed to read chunk");

        // Insert into cache.
        self.chunks.push(new_chunk);
        // Potentially evict if we exceed max_cached_chunks.
        if self.chunks.len() > self.max_cached_chunks {
            let mut evicted = self.chunks.remove(0);
            if evicted.dirty {
                let file_offset = (evicted.inx * self.chunk_size) as u64;
                self.file.seek(SeekFrom::Start(METADATA_SIZE + file_offset)).unwrap();
                let _ = self.file.write_all(&evicted.data);
            }
        }

        let last_idx = self.chunks.len() - 1;
        &mut self.chunks[last_idx]
    }

    // Flush metadata and any dirty chunks to disk.
    fn flush(&mut self) -> io::Result<()> {
        let mut metadata = [0u8; METADATA_SIZE as usize];
        metadata[0..4].copy_from_slice(&(self.chunk_size as u32).to_le_bytes());
        metadata[4..12].copy_from_slice(&(self.num_chunks as u64).to_le_bytes());
        metadata[12..20].copy_from_slice(&(self.head as u64).to_le_bytes());
        metadata[20..28].copy_from_slice(&(self.tail as u64).to_le_bytes());

        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&metadata)?;

        for chunk in &mut self.chunks {
            if chunk.dirty {
                let file_offset = (chunk.inx * self.chunk_size) as u64;
                self.file.seek(SeekFrom::Start(METADATA_SIZE + file_offset))?;
                self.file.write_all(&chunk.data)?;
                chunk.dirty = false;
            }
        }

        Ok(())
    }

    /// Peek data without moving tail.
    /// Returns how many bytes were read into `buf`.
    pub fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let capacity = self.capacity();
        let chunk_size = self.chunk_size;

        let used = self.used();
        let available_for_peek = if self.uncommitted_read > used {
            0
        } else {
            used - self.uncommitted_read
        };
        let to_read = std::cmp::min(available_for_peek, buf.len());

        let mut total_read = 0;
        let mut read_pos = (self.tail + self.uncommitted_read) % capacity;

        while total_read < to_read {
            let chunk_offset = read_pos % chunk_size;
            let available_in_chunk = chunk_size - chunk_offset;
            let read_now = std::cmp::min(available_in_chunk, to_read - total_read);

            let chunk = self.get_chunk(read_pos);
            chunk.read(chunk_offset, &mut buf[total_read..total_read + read_now]);

            total_read += read_now;
            read_pos = (read_pos + read_now) % capacity;
        }

        self.uncommitted_read += total_read;
        Ok(total_read)
    }

    /// Commit read data after sending it, etc.
    pub fn commit_read(&mut self, amount: usize) {
        let commit_amount = std::cmp::min(self.uncommitted_read, amount);
        self.tail = (self.tail + commit_amount) % self.capacity();
        self.uncommitted_read -= commit_amount;
    }

    /// Abort any uncommitted read data so the next peek sees the same data.
    pub fn abort_read(&mut self) {
        self.uncommitted_read = 0;
    }

    /// Force-discard bytes from the buffer.
    /// Moves tail forward, removing old data.
    fn force_discard_bytes(&mut self, mut count: usize) {
        let used = self.used();
        if count > used {
            count = used;
        }
        // If we have uncommitted data, reduce that first.
        if self.uncommitted_read > 0 {
            if count >= self.uncommitted_read {
                count -= self.uncommitted_read;
                self.uncommitted_read = 0;
            } else {
                self.uncommitted_read -= count;
                count = 0;
            }
        }
        self.tail = (self.tail + count) % self.capacity();
    }

    // /// Write an entire LogEntry without partial overwrite.
    // /// If there's not enough free space, discard old data until it fits.
    // pub fn write_entry(&mut self, entry: &LogEntry) -> io::Result<()> {
    //     let serialized = entry.serialize();
    //     let needed = serialized.len();
    //     if needed > self.capacity() {
    //         return Err(io::Error::new(
    //             io::ErrorKind::InvalidInput,
    //             "Record bigger than ring buffer capacity",
    //         ));
    //     }
    //     let free = self.free_space();
    //     if needed > free {
    //         let to_discard = needed - free;
    //         self.force_discard_bytes(to_discard);
    //     }
    //     self.write_all(&serialized)?;
    //     Ok(())
    // }

    // /// Attempt to read one entire LogEntry.
    // /// If not enough data is present to parse the entire record, returns io::ErrorKind::WouldBlock.
    // pub fn read_entry(&mut self) -> io::Result<LogEntry> {
    //     let used = self.used();
    //     if used < 2 { // minimal size check (version alone is 2 bytes)
    //         return Err(io::Error::new(io::ErrorKind::WouldBlock, "Not enough data for even the header"));
    //     }
    //     // We'll read all used bytes via peek, parse from that.
    //     // Then commit exactly how many bytes we consumed.

    //     let mut buf = vec![0u8; used];
    //     let got = self.peek(&mut buf)?; // peek up to 'used' bytes
    //     // got should == used in practice.
    //     if got < 2 {
    //         return Err(io::Error::new(io::ErrorKind::WouldBlock, "Partial data"));
    //     }

    //     // Attempt to deserialize.
    //     use std::io::Cursor;
    //     let mut cursor = Cursor::new(&buf);
    //     match LogEntry::deserialize_from_reader(&mut cursor) {
    //         Ok(entry) => {
    //             // figure out how many bytes were consumed.
    //             let consumed = cursor.position() as usize;
    //             if consumed > got {
    //                 // partial record
    //                 return Err(io::Error::new(io::ErrorKind::WouldBlock, "Partial record"));
    //             }
    //             // commit those consumed bytes so they are removed from the buffer.
    //             self.commit_read(consumed);
    //             Ok(entry)
    //         }
    //         Err(e) => {
    //             if e.kind() == io::ErrorKind::UnexpectedEof {
    //                 // partial record in buffer
    //                 Err(io::Error::new(io::ErrorKind::WouldBlock, "Partial record"))
    //             } else {
    //                 // corrupt => optionally skip or discard
    //                 // for demonstration, discard 1 byte so we don't get stuck.
    //                 self.commit_read(1);
    //                 Err(e)
    //             }
    //         }
    //     }
    // }
}

impl Write for CircleBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let capacity = self.capacity();
        if buf.len() > capacity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Data exceeds total buffer capacity",
            ));
        }

        let mut total_written = 0;
        let mut head = self.head;
        let chunk_size = self.chunk_size;

        while total_written < buf.len() {
            let chunk_offset = head % chunk_size;
            let space_in_chunk = chunk_size - chunk_offset;
            let to_write = std::cmp::min(space_in_chunk, buf.len() - total_written);

            {
                let chunk = self.get_chunk(head);
                chunk.write(chunk_offset, &buf[total_written..total_written + to_write]);
            }

            total_written += to_write;
            head = (head + to_write) % capacity;
        }

        // Overwrite check
        let old_used = if self.head >= self.tail {
            self.head - self.tail
        } else {
            capacity - (self.tail - self.head)
        };
        let new_used = old_used + total_written;
        if new_used > capacity {
            let overwritten = new_used - capacity;
            if self.uncommitted_read > 0 {
                if overwritten >= self.uncommitted_read {
                    self.uncommitted_read = 0;
                } else {
                    self.uncommitted_read -= overwritten;
                }
            }
            self.tail = (self.tail + overwritten) % capacity;
        }

        self.head = head;
        Ok(total_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl Read for CircleBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let capacity = self.capacity();
        let chunk_size = self.chunk_size;
        let used = self.used();
        let to_read = std::cmp::min(used, buf.len());
        let mut total_read = 0;
        let mut tail = self.tail;

        while total_read < to_read {
            let chunk_offset = tail % chunk_size;
            let available_in_chunk = chunk_size - chunk_offset;
            let read_now = std::cmp::min(available_in_chunk, to_read - total_read);

            let chunk = self.get_chunk(tail);
            chunk.read(chunk_offset, &mut buf[total_read..total_read + read_now]);

            total_read += read_now;
            tail = (tail + read_now) % capacity;
        }

        self.tail = tail;
        Ok(total_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::io::{Read, Write};

    #[test]
    fn test_chunked_buffer() -> io::Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("test_buffer");

        // Create buffer with 4 chunks of 4KB each (16KB total)
        let mut buffer = CircleBuffer::new(&path, 4, DEFAULT_CHUNK_SIZE, 2)?;

        // Write 6KB of data (1.5 chunks)
        let data = vec![0xAA; 6144];
        buffer.write_all(&data)?;
        buffer.flush()?;

        // Read back the data with the default read.
        let mut read_buf = vec![0u8; 6144];
        buffer.read_exact(&mut read_buf)?;
        assert_eq!(read_buf, vec![0xAA; 6144]);

        // Now test the peek/commit logic.
        let data2 = b"hello world";
        buffer.write_all(data2)?;
        buffer.flush()?;

        let mut peek_buf = [0u8; 20];
        let peeked = buffer.peek(&mut peek_buf)?;
        assert_eq!(peeked, 11);
        assert_eq!(&peek_buf[..11], b"hello world");

        buffer.commit_read(11);
        let mut empty_buf = [0u8; 20];
        let peeked_empty = buffer.peek(&mut empty_buf)?;
        assert_eq!(peeked_empty, 0);

        Ok(())
    }

    // #[test]
    // fn test_record_read_write() -> io::Result<()> {
    //     let dir = tempdir()?;
    //     let path = dir.path().join("test_records");
    //     let mut buffer = CircleBuffer::new(&path, 4, DEFAULT_CHUNK_SIZE, 2)?;

    //     // Create a sample log entry
    //     let entry = LogEntry {
    //         version: 1,
    //         random: 1234,
    //         timestamp: 987654321,
    //         level: 2,
    //         props: vec![ ("key1".to_string(), "val1".to_string()), ("key2".to_string(), "val2".to_string()) ],
    //         msg: "Hello from the log".to_string(),
    //     };

    //     // Write the entry
    //     buffer.write_entry(&entry)?;
    //     buffer.flush()?;

    //     // Read the entry back
    //     let read_entry = buffer.read_entry()?;
    //     assert_eq!(read_entry.version, entry.version);
    //     assert_eq!(read_entry.random, entry.random);
    //     assert_eq!(read_entry.timestamp, entry.timestamp);
    //     assert_eq!(read_entry.level, entry.level);
    //     assert_eq!(read_entry.props, entry.props);
    //     assert_eq!(read_entry.msg, entry.msg);

    //     // Attempt another read => should block (no data)
    //     let res = buffer.read_entry();
    //     assert!(res.is_err());
    //     assert_eq!(res.err().unwrap().kind(), io::ErrorKind::WouldBlock);

    //     Ok(())
    // }
}