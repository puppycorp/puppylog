use std::io::Cursor;
use std::io::Read;
use std::io::Write;

use chrono::offset;
use chrono::DateTime;
use chrono::NaiveDateTime;
use chrono::Utc;
use puppylog::LogEntry;
use puppylog::LogLevel;
use puppylog::Prop;


#[derive(Debug, Clone, PartialEq)]
pub struct LogHeader {
	pub timestamp: DateTime<Utc>,
	pub data_offset: u64,
	pub data_length: u64,
}

pub struct LogIterator<'a> {
	pub pointers: &'a [LogHeader],
	pub buffer: &'a [u8],
	pub offset: usize,
}

impl<'a> LogIterator<'a> {
	pub fn new(pointers: &'a [LogHeader], buffer: &'a [u8]) -> Self {
		LogIterator {
			pointers,
			buffer,
			offset: 0
		}
	}
}

impl<'a> Iterator for LogIterator<'a> {
	type Item = LogEntry;

	fn next(&mut self) -> Option<Self::Item> {
		if self.offset >= self.pointers.len() {
			return None;
		}

		let pointer = &self.pointers[self.offset];
		self.offset += 1;
		let mut ptr = 0;
		let log = LogEntry::fast_deserialize(&self.buffer[pointer.data_offset as usize..], &mut ptr).unwrap();
	 	Some(log)
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogSegment {
	pub pointers: Vec<LogHeader>,
	pub buffer: Cursor<Vec<u8>>,
}

impl LogSegment {
	pub fn new() -> Self {
		LogSegment {
			pointers: Vec::new(),
			buffer: Cursor::new(Vec::new()),
		}
	}
	
	pub fn iter<'a>(&'a self, end: Option<DateTime<Utc>>) -> LogIterator<'a> {
		let end = end.unwrap_or_else(|| Utc::now());
		let mut offset = 0;

		for pointer in &self.pointers {
			if pointer.timestamp > end {
				continue;
			}
			offset += 1;
		}

		LogIterator::new(&self.pointers[..offset], &self.buffer.get_ref())
	}
	pub fn sort(&mut self) {
		self.pointers.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
	}

	pub fn add_log_entry(&mut self, log: LogEntry) {
		let start = self.buffer.position();
		log.serialize(&mut self.buffer);
		let end = self.buffer.position();
		self.pointers.push(LogHeader {
			timestamp: log.timestamp,
			data_offset: start,
			data_length: end - start,
		});
	}

	pub fn serialize<W: Write>(&self, writer: &mut W) {
		let header_size = self.pointers.len() * std::mem::size_of::<LogHeader>();
		writer.write_all(&header_size.to_be_bytes()).unwrap();
		for pointer in &self.pointers {
			writer.write_all(&pointer.timestamp.timestamp_micros().to_be_bytes()).unwrap();
			writer.write_all(&pointer.data_offset.to_be_bytes()).unwrap();
			writer.write_all(&pointer.data_length.to_be_bytes()).unwrap();
		}
		writer.write_all(&self.buffer.get_ref()).unwrap();
	}

	pub fn parse<R: Read>(reader: &mut R) -> Self {
		let mut buffer = Vec::new();
		let mut pointers = Vec::new();
		let mut header_size = [0u8; 8];
		reader.read_exact(&mut header_size).unwrap();
		let header_size = u64::from_be_bytes(header_size);
		let header_count = header_size / std::mem::size_of::<LogHeader>() as u64;
		let mut header = [0u8; 24];
		for _ in 0..header_count {
            reader.read_exact(&mut header).unwrap();
            let micros = i64::from_be_bytes(header[0..8].try_into().unwrap());
            let timestamp = DateTime::from_timestamp_micros(micros).unwrap();
            let data_offset = u64::from_be_bytes(header[8..16].try_into().unwrap());
            let data_length = u64::from_be_bytes(header[16..24].try_into().unwrap());
            pointers.push(LogHeader {
                timestamp,
                data_offset,
                data_length,
            });
        }
		reader.read_to_end(&mut buffer).unwrap();
		LogSegment {
			pointers,
			buffer: Cursor::new(buffer),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	pub fn test_log_segment() {
		let mut segment = LogSegment::new();
		let log = LogEntry {
			random: 0,
			timestamp: Utc::now(),
			level: LogLevel::Info,
			msg: "Hello, world!".to_string(),
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			..Default::default()
		};
		segment.add_log_entry(log.clone());

		let mut iter = segment.iter(None);
		let lof1 = iter.next().unwrap();
		assert_eq!(log, lof1);
		assert!(iter.next().is_none());

		let mut buff = Vec::new();
		segment.serialize(&mut buff);
		segment.buffer.set_position(0);
		let mut reader = Cursor::new(buff);
		let segment2 = LogSegment::parse(&mut reader);
		assert_eq!(segment, segment2);
	}
}