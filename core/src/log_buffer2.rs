use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

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

            let chunk_size_from_file = u32::from_le_bytes(metadata[0..4].try_into().unwrap());
            let total_chunks_from_file = u64::from_le_bytes(metadata[4..12].try_into().unwrap());
            let head = u64::from_le_bytes(metadata[12..20].try_into().unwrap());
            let tail = u64::from_le_bytes(metadata[20..28].try_into().unwrap());

            // In production, consider validating that chunk_size_from_file == chunk_size
            // and total_chunks_from_file == total_chunks. We'll skip that here.

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
            // naive eviction: remove the first chunk in the vector.
            // you might want a better eviction strategy in real usage.
            let mut evicted = self.chunks.remove(0);
            // flush any dirty chunk.
            if evicted.dirty {
                let file_offset = (evicted.inx * self.chunk_size) as u64;
                self.file.seek(SeekFrom::Start(METADATA_SIZE + file_offset)).unwrap();
                let _ = self.file.write_all(&evicted.data);
            }
        }

        // Return the newly inserted chunk.
        let last_idx = self.chunks.len() - 1;
        &mut self.chunks[last_idx]
    }

    // Flush metadata and any dirty chunks to disk.
    fn flush(&mut self) -> io::Result<()> {
        // Write updated metadata (head, tail, etc).
        let mut metadata = [0u8; METADATA_SIZE as usize];
        metadata[0..4].copy_from_slice(&(self.chunk_size as u32).to_le_bytes());
        metadata[4..12].copy_from_slice(&(self.num_chunks as u64).to_le_bytes());
        metadata[12..20].copy_from_slice(&(self.head as u64).to_le_bytes());
        metadata[20..28].copy_from_slice(&(self.tail as u64).to_le_bytes());

        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&metadata)?;

        // Flush dirty chunks.
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

    /// Peek data from the buffer without moving the tail pointer.
    /// This method reads up to `buf.len()` or however many bytes are available
    /// but does not commit them as consumed. The next call to `peek` or `commit_read`
    /// will continue from where we left off.
    ///
    /// Return value is how many bytes were actually read.
    pub fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let capacity = self.capacity();
        let chunk_size = self.chunk_size;

        // How many bytes are in the buffer currently?
        let used = if self.head >= self.tail {
            self.head - self.tail
        } else {
            capacity - (self.tail - self.head)
        };

        // We can only read up to what is available minus what we've already peeked.
        let available_for_peek = if self.uncommitted_read > used {
            // Something's off if uncommitted_read > used, but let's handle gracefully.
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

        // We increase the uncommitted_read by the amount we actually read.
        self.uncommitted_read += total_read;
        Ok(total_read)
    }

    /// Commit read data after we have successfully processed/sent it.
    /// Moves the tail pointer forward by `amount`, effectively discarding that data from the ring.
    pub fn commit_read(&mut self, amount: usize) {
        // We only allow committing up to uncommitted_read.
        let commit_amount = std::cmp::min(self.uncommitted_read, amount);
        self.tail = (self.tail + commit_amount) % self.capacity();
        self.uncommitted_read -= commit_amount;
    }

    /// Abort any uncommitted read data. This will reset the uncommitted_read to 0.
    /// The next peek would re-read the same data.
    pub fn abort_read(&mut self) {
        self.uncommitted_read = 0;
    }
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

            // Write into chunk.
            {
                let chunk = self.get_chunk(head);
                chunk.write(chunk_offset, &buf[total_written..total_written + to_write]);
            }

            total_written += to_write;
            head = (head + to_write) % capacity;
        }

        // Compute how much data was in the buffer before.
        let old_used = if self.head >= self.tail {
            self.head - self.tail
        } else {
            capacity - (self.tail - self.head)
        };
        let new_used = old_used + total_written;
        if new_used > capacity {
            // we've overwritten some data, so move tail forward.
            let overwritten = new_used - capacity;
            // If there's uncommitted reads, reduce them accordingly if they are overwritten.
            // If uncommitted_read remains referencing data we've overwritten, we should adjust it.
            // We'll do so carefully.
            if self.uncommitted_read > 0 {
                // If overwritten >= uncommitted_read, that means all uncommitted data is lost.
                // We'll reset uncommitted_read to 0. Otherwise, we reduce it.
                if overwritten >= self.uncommitted_read {
                    self.uncommitted_read = 0;
                } else {
                    self.uncommitted_read -= overwritten;
                }
            }
            // Now move the tail forward.
            self.tail = (self.tail + overwritten) % capacity;
        }

        // Now update head.
        self.head = head;

        Ok(total_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl Read for CircleBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // The default read automatically commits, so we can do the same logic
        // as a normal ring read: read from tail up to used, and advance tail.
        let capacity = self.capacity();
        let chunk_size = self.chunk_size;

        // Compute how many bytes are currently in the buffer.
        let used = if self.head >= self.tail {
            self.head - self.tail
        } else {
            capacity - (self.tail - self.head)
        };

        // We can only read up to 'used' bytes.
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
        // This read automatically commits (like the old behavior).
        let mut read_buf = vec![0u8; 6144];
        buffer.read_exact(&mut read_buf)?;
        assert_eq!(read_buf, vec![0xAA; 6144]);

        // Now test the peek/commit logic.
        // Write some new data.
        let data2 = b"hello world";
        buffer.write_all(data2)?;
        buffer.flush()?;

        // Instead of the normal read, let's use peek.
        let mut peek_buf = [0u8; 20];
        let peeked = buffer.peek(&mut peek_buf)?;
        assert_eq!(peeked, 11); // length of "hello world"
        assert_eq!(&peek_buf[..11], b"hello world");

        // Now we commit.
        buffer.commit_read(11);

        // If we peek again, we should see 0 bytes (because we consumed them).
        let mut empty_buf = [0u8; 20];
        let peeked_empty = buffer.peek(&mut empty_buf)?;
        assert_eq!(peeked_empty, 0);

        Ok(())
    }
}
