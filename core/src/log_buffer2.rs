use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const METADATA_SIZE: u64 = 32;
const DEFAULT_CHUNK_SIZE: usize = 4096; // 4KB

struct Chunk {
	inx: usize,
	dirty: bool,
	data: Vec<u8>,
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
		dst.copy_from_slice(&self.data[offset..(offset + dst.len())])
	}
}

pub struct CircleBuffer {
    file: File,
    chunk_size: usize,
    num_chunks: usize,
    head: usize,
    tail: usize,
    max_cached_chunks: usize,
	chunks: Vec<Chunk>
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

        let file_size = METADATA_SIZE + (chunk_size as u64) * total_chunks as u64;
        file.set_len(file_size)?;

        let (head, tail) = if file.metadata()?.len() >= METADATA_SIZE {
            let mut metadata = [0u8; METADATA_SIZE as usize];
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut metadata)?;
            
            let chunk_size_from_file = u32::from_le_bytes(metadata[0..4].try_into().unwrap());
            let total_chunks_from_file = u64::from_le_bytes(metadata[4..12].try_into().unwrap());
            let head = u64::from_le_bytes(metadata[12..20].try_into().unwrap());
            let tail = u64::from_le_bytes(metadata[20..28].try_into().unwrap());

            // if chunk_size_from_file != chunk_size as u32 || total_chunks_from_file != total_chunks as u64 {
            //     return Err(io::Error::new(
            //         io::ErrorKind::InvalidData,
            //         "Existing file parameters don't match requested configuration",
            //     ));
            // }

            (head, tail)
        } else {
            let mut metadata = [0u8; METADATA_SIZE as usize];
            metadata[0..4].copy_from_slice(&(chunk_size as u32).to_le_bytes());
            metadata[4..12].copy_from_slice(&total_chunks.to_le_bytes());
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
            chunks: Vec::with_capacity(10)
        })
    }

	fn capacity(&self) -> usize {
		self.num_chunks * self.chunk_size
	}

	fn get_chunk(&mut self, offset: usize) -> &mut Chunk {
		let inx = if offset > 0 { self.capacity() / offset } else { 0 };
	
		// Try to find the position of the chunk.
		if let Some(pos) = self.chunks.iter().position(|p| p.inx == inx) {
			return &mut self.chunks[pos];
		}
	
		// If not found, load a new chunk.
		let file_offset = inx * self.chunk_size;
		self.file.seek(SeekFrom::Start(METADATA_SIZE + file_offset as u64))
			.expect("failed to seek to offset");
		
		let mut new_chunk = Chunk::new(inx, self.chunk_size);
		self.file.read(&mut new_chunk.data[..self.chunk_size])
			.expect("failed to read chunk data");
	
		// Now that there is no outstanding borrow on self.chunks, we can push.
		self.chunks.push(new_chunk);
		
		// And safely return a mutable reference to the newly inserted chunk.
		let last_index = self.chunks.len() - 1;
		&mut self.chunks[last_index]
	}

	fn flush(&mut self) -> io::Result<()> {
		let mut metadata = [0u8; METADATA_SIZE as usize];
        metadata[0..4].copy_from_slice(&(self.chunk_size as u32).to_le_bytes());
        metadata[4..12].copy_from_slice(&self.num_chunks.to_le_bytes());
        metadata[12..20].copy_from_slice(&self.head.to_le_bytes());
        metadata[20..28].copy_from_slice(&self.tail.to_le_bytes()); 
        self.file.seek(SeekFrom::Start(METADATA_SIZE))?;
        self.file.write_all(&metadata)?;

		let capacity = self.capacity();
		for chunk in &mut self.chunks {
			if !chunk.dirty { continue; }
			let offset = capacity / chunk.inx;
			self.file.seek(SeekFrom::Start(METADATA_SIZE + offset as u64));
			self.file.write_all(&chunk.data);
			chunk.dirty = false;
		}

		Ok(())
	}
}

impl Write for CircleBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		let capacity = self.capacity();
		if buf.len() > capacity {
			return Err(io::Error::new(
				io::ErrorKind::InvalidInput,
				"Data exceeds buffer capacity",
			));
		}
		let chunk_size = self.chunk_size;
		let mut head = self.head;
        let mut bytes_written = 0;
		while bytes_written < buf.len() {
			let chunk_offset = head % chunk_size;
			let to_write = std::cmp::min(buf.len(), chunk_size - chunk_offset);
			if head + to_write > self.tail {
				let add_amount = head + to_write + 1;
				if add_amount > capacity {
					self.tail = 1;
				} else {
					self.tail = head + to_write + 1;
				}
			}
			let chunk = self.get_chunk(head);
			chunk.write(chunk_offset as usize, &buf[bytes_written..(bytes_written + to_write)]);
			bytes_written += to_write;
			head += to_write;
			if head == capacity { head = 0; }
		}
		self.head = head;
        Ok(bytes_written)
    }

    fn flush(&mut self) -> io::Result<()> {
		self.flush()
    }
}

impl Read for CircleBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		let capacity = self.capacity();
		if buf.len() > capacity {
			return Err(io::Error::new(
				io::ErrorKind::InvalidInput,
				"Cannot read more than capacity of buffer"
			));
		}
		let chunk_size = self.chunk_size;
		let mut tail = self.tail;
		let mut bytes_read = 0;
		let toberead = if self.head > tail { std::cmp::min(buf.len(), self.head - tail) } 
		else { std::cmp::min(capacity - tail + self.head, buf.len()) };
		while bytes_read < buf.len() {
			let chunk_offset = tail % chunk_size;
			let toread = std::cmp::min(toberead, chunk_size - chunk_offset);
			let chunk = self.get_chunk(tail);
			chunk.read(chunk_offset, &mut buf[bytes_read..(bytes_read + toread)]);
			bytes_read += toread;
			tail = (tail + toread) % self.num_chunks;
		}
		self.tail = tail;
        Ok(toberead)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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

        // Read back the data
        let mut read_buf = vec![0u8; 6144];
        buffer.read_exact(&mut read_buf)?;
        assert_eq!(read_buf, vec![0xAA; 6144]);

        // Verify cache behavior
        // assert_eq!(buffer.cache.len(), 2); // Should have cached the last 2 chunks
        // assert!(buffer.cache.contains(&1)); // Tail chunk
        // assert!(buffer.cache.contains(&2)); // Next chunk

        Ok(())
    }
}