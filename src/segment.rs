use chrono::DateTime;
use chrono::Utc;
use puppylog::LogEntry;
use puppylog::LogentryDeserializerError;
use serde::Serialize;
use std::cmp::Ordering;
use std::io::Read;
use std::io::Write;
use zstd::Encoder;

#[derive(Debug)]
pub struct LogIterator<'a> {
	pub buffer: &'a [LogEntry],
	pub offset: usize,
}

impl<'a> LogIterator<'a> {
	pub fn new(buffer: &'a [LogEntry], offset: usize) -> Self {
		LogIterator { buffer, offset }
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
	pub device_id: Option<String>,
	pub first_timestamp: DateTime<Utc>,
	pub last_timestamp: DateTime<Utc>,
	pub original_size: usize,
	pub compressed_size: usize,
	pub logs_count: u64,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogSegment {
	pub buffer: Vec<LogEntry>,
}

impl LogSegment {
	pub fn with_logs(mut logs: Vec<LogEntry>) -> Self {
		logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
		LogSegment { buffer: logs }
	}
	pub fn new() -> Self {
		LogSegment {
			buffer: Vec::with_capacity(500_000),
		}
	}
	pub fn iter(&self) -> LogIterator {
		let i = self.buffer.len();
		LogIterator::new(&self.buffer[..i], i)
	}
	fn date_index(&self, date: DateTime<Utc>) -> usize {
		self.buffer
			.binary_search_by(|log| {
				if log.timestamp > date {
					Ordering::Greater
				} else {
					Ordering::Less
				}
			})
			.unwrap_or_else(|idx| idx)
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
		use std::io::ErrorKind;

		let mut header = [0u8; HEADER_SIZE];
		if let Err(err) = reader.read_exact(&mut header) {
			log::error!("failed to read segment header: {}", err);
			return LogSegment { buffer: Vec::new() };
		}
		let magic = String::from_utf8_lossy(&header[0..11]);
		if magic != MAGIC {
			panic!("Invalid magic: {}", magic);
		}
		let version = u16::from_be_bytes(header[11..13].try_into().unwrap());
		if version != VERSION {
			panic!("Invalid version: {}", version);
		}
		let mut log_entries = Vec::new();
		let mut buff = Vec::new();
		let mut ptr = 0;
		match reader.read_to_end(&mut buff) {
			Ok(_) => {}
			Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
				log::warn!("truncated segment: {}", err);
			}
			Err(err) => {
				log::error!("failed to read segment: {}", err);
				return LogSegment { buffer: Vec::new() };
			}
		}
		loop {
			match LogEntry::fast_deserialize(&buff, &mut ptr) {
				Ok(log_entry) => log_entries.push(log_entry),
				Err(LogentryDeserializerError::NotEnoughData) => break,
				Err(err) => panic!("Error deserializing log entry: {:?}", err),
			}
		}
		LogSegment {
			buffer: log_entries,
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

pub fn compress_segment(buf: &[u8]) -> anyhow::Result<Vec<u8>> {
	let mut encoder = Encoder::new(Vec::new(), 14)?;
	encoder.multithread(num_cpus::get() as u32)?;
	encoder.write_all(&buf)?;
	Ok(encoder.finish()?)
}

#[cfg(test)]
mod tests {
	use std::io::Cursor;

	use super::*;
	use puppylog::LogLevel;
	use puppylog::Prop;
	use std::io::Write;
	use zstd::stream::{Decoder, Encoder};

	#[test]
	pub fn test_log_segment() {
		let mut segment = LogSegment::new();
		let timestamp = DateTime::from_timestamp_micros(1740074054 * 1_000_000).unwrap();
		let log = LogEntry {
			random: 0,
			timestamp,
			level: LogLevel::Info,
			msg: "Hello, world!".to_string(),
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			..Default::default()
		};
		segment.add_log_entry(log.clone());

		let mut iter = segment.iter();
		let lof1 = iter.next().unwrap();
		assert_eq!(log, lof1.clone());
		assert!(iter.next().is_none());

		let mut buff = Vec::new();
		segment.serialize(&mut buff);
		let mut reader = Cursor::new(buff);
		let segment2 = LogSegment::parse(&mut reader);
		assert_eq!(segment, segment2);
	}

	#[test]
	pub fn parse_truncated_does_not_panic() {
		let mut segment = LogSegment::new();
		let timestamp = DateTime::from_timestamp_micros(1740074054 * 1_000_000).unwrap();
		segment.add_log_entry(LogEntry {
			random: 0,
			timestamp,
			level: LogLevel::Info,
			msg: "Hello".to_string(),
			props: vec![],
			..Default::default()
		});

		let mut plain = Vec::new();
		segment.serialize(&mut plain);

		let mut enc = Encoder::new(Vec::new(), 0).unwrap();
		enc.write_all(&plain).unwrap();
		let encoded = enc.finish().unwrap();
		let truncated = &encoded[..encoded.len() - 1];

		let cursor = Cursor::new(truncated.to_vec());
		let mut dec = Decoder::new(cursor).unwrap();
		let _ = LogSegment::parse(&mut dec);
	}
}
