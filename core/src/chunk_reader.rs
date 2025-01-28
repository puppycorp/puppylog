use std::collections::VecDeque;
use std::io::Read;
use bytes::Bytes;

#[derive(Debug)]
pub struct ChunkReader {
    chunks: VecDeque<Bytes>,
    current_position: ChunkPosition,
    committed_position: ChunkPosition,
}

#[derive(Debug, Clone, Copy)]
struct ChunkPosition {
    chunk_index: usize,
    offset: usize,
}

impl ChunkPosition {
    fn new() -> Self {
        Self {
            chunk_index: 0,
            offset: 0,
        }
    }
}

impl ChunkReader {
    pub fn new() -> Self {
        ChunkReader {
            chunks: VecDeque::new(),
            current_position: ChunkPosition::new(),
            committed_position: ChunkPosition::new(),
        }
    }

    pub fn add_chunk(&mut self, chunk: Bytes) {
        log::debug!("Adding chunk of {} bytes", chunk.len());
        self.chunks.push_back(chunk);
    }

    pub fn commit(&mut self) {
        self.committed_position = self.current_position;
        
        // Remove fully read chunks
        if self.should_remove_chunks() {
            self.remove_processed_chunks();
        }
        
        log::debug!(
            "Committed. Remaining chunks: {}, Current offset: {}", 
            self.chunks.len(), 
            self.current_position.offset
        );
    }

    pub fn rollback(&mut self) {
        log::debug!("Rolling back to previous committed position");
        self.current_position = self.committed_position;
    }

    fn should_remove_chunks(&self) -> bool {
        self.current_position.offset == self.current_chunk_size()
    }

    fn remove_processed_chunks(&mut self) {
        let chunks_to_remove = self.current_position.chunk_index + 1;
        self.chunks.drain(0..chunks_to_remove);
        self.current_position.chunk_index = 0;
        self.current_position.offset = 0;
        self.committed_position = self.current_position;
    }

    fn current_chunk_size(&self) -> usize {
        self.chunks
            .get(self.current_position.chunk_index)
            .map_or(0, |chunk| chunk.len())
    }

    fn has_more_data(&self) -> bool {
        self.current_position.chunk_index < self.chunks.len()
    }

    fn advance_to_next_chunk(&mut self) -> bool {
        self.current_position.chunk_index += 1;
        self.current_position.offset = 0;
        self.has_more_data()
    }
}

impl Read for ChunkReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        log::debug!("Reading up to {} bytes", buf.len());
        let mut bytes_read = 0;

        while bytes_read < buf.len() && self.has_more_data() {
            if self.current_position.offset >= self.current_chunk_size() {
                if !self.advance_to_next_chunk() {
                    break;
                }
                continue;
            }

            let chunk = &self.chunks[self.current_position.chunk_index];
            let bytes_to_read = std::cmp::min(
                chunk.len() - self.current_position.offset,
                buf.len() - bytes_read
            );

            let start = self.current_position.offset;
            let end = start + bytes_to_read;
            
            buf[bytes_read..bytes_read + bytes_to_read]
                .copy_from_slice(&chunk[start..end]);

            bytes_read += bytes_to_read;
            self.current_position.offset += bytes_to_read;
        }

        log::debug!("Read {} bytes", bytes_read);
        Ok(bytes_read)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use bytes::Bytes;
	use crate::{LogEntry, LogLevel, Prop};
	use super::*;

	#[test]
	fn parse_chucks() {
		let chunck1 = Bytes::from_static(b"Hello");
		let chunck2 = Bytes::from_static(b" World");
		let mut reader = super::ChunkReader::new();
		reader.add_chunk(chunck1);
		reader.add_chunk(chunck2);

		let mut buf = [0; 11];
		
		let read = reader.read(&mut buf).unwrap();
		println!("{:?}", buf);
		assert_eq!(read, 11);
		assert_eq!(&buf, b"Hello World");
	}

	#[test]
	fn can_continue_parsing_when_new_data_arrives() {
		let chunck1 = Bytes::from_static(b"Hello");
		let mut reader = super::ChunkReader::new();
		reader.add_chunk(chunck1);
		let mut buf = [0; 5];
		
		let read = reader.read(&mut buf).unwrap();
		println!("{:?}", buf);
		assert_eq!(read, 5);
		assert_eq!(&buf, b"Hello");
		println!("before commit {:?}", reader);
		reader.commit();
		println!("after commit {:?}", reader);

		let mut buf = [0; 6];
		let chunck2 = Bytes::from_static(b" World");
		reader.add_chunk(chunck2);
		let read = reader.read(&mut buf).unwrap();
		println!("{:?}", buf);
		assert_eq!(read, 6);
		assert_eq!(&buf, b" World");
	}


	#[test]
	fn next_chuck_comes_in_the_middle_of_reading() {
		let chunck1 = Bytes::from_static(b"Hello");
		let mut buff = [0; 11];
		let mut reader = super::ChunkReader::new();
		reader.add_chunk(chunck1);
		let res = reader.read(&mut buff);
		println!("{:?}", res);
		println!("{:?}", reader);
		assert_eq!(res.unwrap(), 5);
		reader.rollback();
		let chunck2 = Bytes::from_static(b" World");
		reader.add_chunk(chunck2);
		let res = reader.read(&mut buff);
		println!("{:?}", res);
		println!("{:?}", buff);
		assert_eq!(res.unwrap(), 11);
		assert_eq!(&buff, b"Hello World");
	}

	#[test]
	fn only_read_part() {
		let chunck1 = Bytes::from_static(b"Hello world");
		let mut buff = [0; 5];
		let mut reader = super::ChunkReader::new();
		reader.add_chunk(chunck1);
		let res = reader.read(&mut buff);
		println!("{:?}", res);
		println!("{:?}", reader);
		assert_eq!(res.unwrap(), 5);
		assert_eq!(&buff, b"Hello");
		reader.commit();
		let res = reader.read(&mut buff);
		println!("{:?}", res);
		println!("{:?}", buff);
		assert_eq!(res.unwrap(), 5);
	}

	#[test]
	fn dunnoooo() {
		let chunck1 = Bytes::from_static(b"Hello");
		let mut reader = super::ChunkReader::new();
		reader.add_chunk(chunck1);

		let mut buf = [0; 2];
		let read = reader.read(&mut buf).unwrap();
		assert_eq!(read, 2);
		assert_eq!(&buf, b"He");
		let mut buf = [0; 3];
		let read = reader.read(&mut buf).unwrap();
		assert_eq!(read, 3);
		assert_eq!(&buf, b"llo");
	}
}