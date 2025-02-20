use std::cmp::Ordering;
use std::io::Read;
use std::io::Write;
use std::sync::Arc;
use bytes::Bytes;
use chrono::DateTime;
use chrono::Utc;
use puppylog::LogEntry;
use puppylog::LogEntryChunkParser;
use serde::Serialize;

#[derive(Debug)]
pub struct LogIterator<'a> {
	pub buffer: &'a [LogEntry],
	pub offset: usize,
}

impl<'a> LogIterator<'a> {
	pub fn new(buffer: &'a [LogEntry], offset: usize) -> Self {
		LogIterator {
			buffer,
			offset
		}
	}
}

impl<'a> Iterator for LogIterator<'a> {
	type Item = &'a LogEntry;

	fn next(&mut self) -> Option<Self::Item> {
		if self.offset == 0 {
			return None;
		}
		let log = &self.buffer[self.offset - 1];
		self.offset -= 1;
		Some(log)
	}
}

pub const MAGIC: &str = "PUPPYLOGSEG";
pub const VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 13;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentMeta {
	pub id: u32,
	pub first_timestamp: DateTime<Utc>,
	pub last_timestamp: DateTime<Utc>,
	pub original_size: usize,
	pub compressed_size: usize,
	pub logs_count: u64,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogSegment {
	pub buffer: Vec<LogEntry>
}

impl LogSegment {
	pub fn with_logs(mut logs: Vec<LogEntry>) -> Self {
		logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
		LogSegment {
			buffer: logs
		}
	}
	pub fn new() -> Self {
		LogSegment {
			buffer: Vec::new()
		}
	}
	pub fn iter(&self, end: DateTime<Utc>) -> LogIterator {
		let i = self.date_index(end);
		LogIterator::new(&self.buffer[..i], i)
	}
	fn date_index(&self, date: DateTime<Utc>) -> usize {
		self.buffer.binary_search_by(|log| {
			if log.timestamp > date {
				Ordering::Greater
			} else {
				Ordering::Less
			}
		}).unwrap_or_else(|idx| idx)
	}
	pub fn sort(&mut self) {
		self.buffer.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
	}

	pub fn add_log_entry(&mut self, log: LogEntry) {
		self.buffer.push(log);
	}

	pub fn serialize<W: Write>(&self, writer: &mut W) {
		writer.write_all(MAGIC.as_bytes()).unwrap();
		writer.write_all(&VERSION.to_be_bytes()).unwrap();
		for log in &self.buffer {
			log.serialize(writer);
		}
	}

	pub fn parse<R: Read>(reader: &mut R) -> Self {
		let mut header = [0u8; HEADER_SIZE];
		reader.read_exact(&mut header).unwrap();
		let magic = String::from_utf8_lossy(&header[0..11]);
		if magic != MAGIC {
			panic!("Invalid magic: {}", magic);
		}
		let version = u16::from_be_bytes(header[11..13].try_into().unwrap());
		if version != VERSION {
			panic!("Invalid version: {}", version);
		}
		let mut chunk_parser = LogEntryChunkParser::new();
		let mut chunk = [0u8; 4096];
		loop {
			let n = reader.read(&mut chunk).unwrap();
			if n == 0 {
				break;
			}
			chunk_parser.add_chunk(Bytes::copy_from_slice(&chunk[..n]));
		}

		LogSegment {
			buffer: chunk_parser.log_entries
		}
	}

	pub fn contains_date(&self, date: DateTime<Utc>) -> bool {
		if self.buffer.is_empty() {
			return false;
		}
		let first = self.buffer.first().unwrap();
		date >= first.timestamp
	}
}

#[cfg(test)]
mod tests {
	use std::io::Cursor;
	use chrono::Duration;
	use chrono::NaiveDate;
use chrono::TimeZone;
use puppylog::LogLevel;
	use puppylog::Prop;
	use super::*;

	#[test]
	pub fn test_log_segment() {
		let mut segment = LogSegment::new();
		let log = LogEntry {
			random: 0,
			timestamp: Utc.ymd(2025, 2, 20).and_hms(0, 0, 0),
			level: LogLevel::Info,
			msg: "Hello, world!".to_string(),
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			..Default::default()
		};
		segment.add_log_entry(log.clone());

		let mut iter = segment.iter(Utc::now() + Duration::days(1));
		let lof1 = iter.next().unwrap();
		assert_eq!(log, lof1.clone());
		assert!(iter.next().is_none());

		let mut buff = Vec::new();
		segment.serialize(&mut buff);
		let mut reader = Cursor::new(buff);
		let segment2 = LogSegment::parse(&mut reader);
		assert_eq!(segment, segment2);
	}
}