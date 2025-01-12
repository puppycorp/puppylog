mod logfile;

use std::io;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use byteorder::LittleEndian;
use chrono::DateTime;
use chrono::Utc;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub enum LogLevel {
	Debug,
	Info,
	Warn,
	Error
}

impl Into<u8> for &LogLevel {
	fn into(self) -> u8 {
		match self {
			LogLevel::Debug => 0,
			LogLevel::Info => 1,
			LogLevel::Warn => 2,
			LogLevel::Error => 3,
		}
	}
}

impl From<u8> for LogLevel {
	fn from(value: u8) -> Self {
		match value {
			0 => LogLevel::Debug,
			1 => LogLevel::Info,
			2 => LogLevel::Warn,
			3 => LogLevel::Error,
			_ => panic!("Invalid log level")
		}
	}
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
	pub timestamp: DateTime<Utc>,
	pub level: LogLevel,
	pub props: Vec<(String, String)>,
	pub msg: String
}

impl LogEntry {
	pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
		writer.write_i64::<LittleEndian>(self.timestamp.timestamp_millis())?;
		writer.write_u8((&self.level).into())?;
		writer.write_u8(self.props.len() as u8)?;
		for (key, value) in &self.props {
			writer.write_u8(key.len() as u8)?;
			writer.write_all(key.as_bytes())?;
			writer.write_u8(value.len() as u8)?;
			writer.write_all(value.as_bytes())?;
		}
		writer.write_u32::<LittleEndian>(self.msg.len() as u32)?;
		writer.write_all(self.msg.as_bytes())?;
		Ok(())
	}

	pub fn  deserialize<R: Read>(reader: &mut R) -> io::Result<LogEntry> {
		let timestamp = reader.read_i64::<LittleEndian>()?;
		let secs = timestamp / 1000;
		let nanos = ((timestamp % 1000) * 1_000_000) as u32;
		let timestamp = match DateTime::from_timestamp(secs, nanos) {
			Some(dt) => dt,
			None => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid timestamp"))
		};
		let level = reader.read_u8()?;
		let level = LogLevel::from(level);
		let prop_count = reader.read_u8()?;
		let mut props = vec![];
		for _ in 0..prop_count {
			let key_len = reader.read_u8()?;
			let mut key = vec![0; key_len as usize];
			reader.read_exact(&mut key)?;
			let key = String::from_utf8_lossy(&key);
			let value_len = reader.read_u8()?;
			let mut value = vec![0; value_len as usize];
			reader.read_exact(&mut value)?;
			let value = String::from_utf8_lossy(&value).to_string();
			props.push((key.to_string(), value));
		}

		let msg_len = reader.read_u32::<LittleEndian>()?;
		let mut msg = vec![0; msg_len as usize];
		reader.read_exact(&mut msg)?;
		let msg = String::from_utf8(msg).unwrap();
		Ok(LogEntry {
			timestamp,
			level,
			props,
			msg
		})
	}
}

pub struct LogEntryParser {
	buffer: Cursor<Vec<u8>>,
	filled: usize,
    pub log_entries: Vec<LogEntry>,
}

impl LogEntryParser {
    pub fn new() -> Self {
        LogEntryParser {
            log_entries: Vec::new(),
			buffer: Cursor::new(vec![]),
			filled: 0
        }
    }

    pub fn parse(&mut self, data: &[u8]) {
		if self.filled + data.len() > self.buffer.get_ref().len() {
			let new_capacity = (self.buffer.get_ref().len() + data.len()).max(self.buffer.get_ref().len() * 2);
			log::info!("Resizing buffer to {}", new_capacity);
			self.buffer.get_mut().resize(new_capacity, 0);
			self.buffer.set_position(self.filled as u64);
		}

		self.buffer.get_mut()[self.filled..self.filled + data.len()].copy_from_slice(data);
		self.filled += data.len();
		while let Ok(entry) = LogEntry::deserialize(&mut self.buffer) {
            self.log_entries.push(entry);
        }
		log::info!("filled: {}", self.filled);
		log::info!("pos: {}", self.buffer.position());

		if self.buffer.position() as usize > self.buffer.get_ref().len() / 2 {
			log::info!("Moving buffer to the left");
			let pos = self.buffer.position() as usize;
			log::info!("pos: {}", pos);
			log::info!("filled: {}", self.filled);
			self.buffer.get_mut().copy_within(pos..self.filled, 0);
			self.filled -= pos;
			self.buffer.set_position(0);
		}
    }
}

fn move_to_left(buffer: &mut Vec<u8>, offset: usize) {
	for i in 0..buffer.len() - offset {
		buffer[i] = buffer[i + offset];
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn test_serialize_and_deserialize() {
		use std::io::Cursor;
		use chrono::Utc;
		use super::{LogEntry, LogLevel};

		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![
				("key1".to_string(), "value1".to_string()),
				("key2".to_string(), "value2".to_string())
			],
			msg: "Hello, world!".to_string()
		};

		let mut buffer = Cursor::new(vec![]);
		entry.serialize(&mut buffer).unwrap();
		buffer.set_position(0);
		let deserialized = LogEntry::deserialize(&mut buffer).unwrap();

		assert_eq!(entry.timestamp.timestamp_millis(), deserialized.timestamp.timestamp_millis());
		assert_eq!(entry.level, deserialized.level);
		assert_eq!(entry.props, deserialized.props);
		assert_eq!(entry.msg, deserialized.msg);
	}

	#[test]
	fn test_serialize_and_logentry_parser() {
		use super::{LogEntry, LogLevel};
		use std::io::Cursor;
		use chrono::Utc;

		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![
				("key1".to_string(), "value1".to_string()),
				("key2".to_string(), "value2".to_string())
			],
			msg: "Hello, world!".to_string()
		};

		let mut buffer = Cursor::new(vec![]);
		entry.serialize(&mut buffer).unwrap();
		buffer.set_position(0);

		let mut parser = super::LogEntryParser::new();
		let mut entries = vec![];
		parser.parse(&buffer.get_ref(), |entry| entries.push(entry));

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].timestamp.timestamp_millis(), entry.timestamp.timestamp_millis());
		assert_eq!(entries[0].level, entry.level);
		assert_eq!(entries[0].props, entry.props);
		assert_eq!(entries[0].msg, entry.msg);
	}
}