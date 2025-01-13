use std::collections::VecDeque;
use std::io::Read;
use bytes::Bytes;

#[derive(Debug)]
pub struct ChunckReader {
	chunks: VecDeque<Bytes>,
	offset: usize,
	noffset: usize,
	chunk: usize
}

impl ChunckReader {
	pub fn new() -> Self {
		ChunckReader {
			chunks: VecDeque::new(),
			offset: 0,
			noffset: 0,
			chunk: 0
		}
	}

	pub fn add_chunk(&mut self, chunck: Bytes) {
		self.chunks.push_back(chunck);
	}

	pub fn commit(&mut self) {
		self.offset = self.noffset;
		let drain_amount = if self.chunks[self.chunk].len() == self.noffset {
			self.noffset = 0;
			self.chunk + 1
		} else {
			self.chunk
		};
		self.chunks.drain(0..drain_amount);
		self.chunk = 0;
		log::info!("chuncks count: {:?}", self.chunks.len());
	}

	pub fn rollback(&mut self) {
		self.noffset = self.offset;
		self.chunk = 0;
	}
}

impl Read for ChunckReader {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		let mut read = 0;
		while read < buf.len() {
			if self.chunk >= self.chunks.len() {
				log::info!("No more data to read");
				log::info!("self.chunk: {}", self.chunk);
				log::info!("self.chunks.len(): {}", self.chunks.len());
				break;
			}
			if self.noffset >= self.chunks[self.chunk].len() {
				self.chunk += 1;
				if self.chunk >= self.chunks.len() {
					break;
				}
				self.noffset = 0;
			}
			let read_len = (self.chunks[self.chunk].len() - self.noffset).min(buf.len() - read);
			buf[read..read + read_len].copy_from_slice(&self.chunks[self.chunk][self.noffset..self.noffset + read_len]);
			read += read_len;
			self.noffset += read_len;
		}
		Ok(read)
	}
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use bytes::Bytes;

	#[test]
	fn parse_chucks() {
		let chunck1 = Bytes::from_static(b"Hello");
		let chunck2 = Bytes::from_static(b" World");
		let mut reader = super::ChunckReader::new();
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
		let mut reader = super::ChunckReader::new();
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
		let mut reader = super::ChunckReader::new();
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
		let mut reader = super::ChunckReader::new();
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
		let mut reader = super::ChunckReader::new();
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