use std::io::{self, Read, Write};
use byteorder::LittleEndian;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::Serialize;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;

use crate::ChunkReader;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LogLevel {
	Trace,
	Debug,
	Info,
	Warn,
	Error,
	Fatal,
	Uknown
}

impl LogLevel {
	pub fn from_string(value: &str) -> Self {
		match value {
			"trace" | "TRACE" => LogLevel::Trace,
			"debug" | "DEBUG" => LogLevel::Debug,
			"info" | "INFO" => LogLevel::Info,
			"warn" | "WARN" => LogLevel::Warn,
			"error" | "ERROR" => LogLevel::Error,
			"fatal" | "FATAL" => LogLevel::Fatal,
			_ => LogLevel::Uknown
		}
	}

	pub fn from_i64(value: i64) -> Self {
		match value {
			1 => LogLevel::Trace,
			2 => LogLevel::Debug,
			3 => LogLevel::Info,
			4 => LogLevel::Warn,
			5 => LogLevel::Error,
			6 => LogLevel::Fatal,
			_ => LogLevel::Uknown
		}
	}
}

impl Into<u8> for &LogLevel {
	fn into(self) -> u8 {
		match self {
			LogLevel::Trace => 1,
			LogLevel::Debug => 2,
			LogLevel::Info => 3,
			LogLevel::Warn => 4,
			LogLevel::Error => 5,
			LogLevel::Fatal => 6,
			LogLevel::Uknown => 0
		}
	}
}

impl TryFrom<u8> for LogLevel {
	type Error = &'static str;

	fn try_from(value: u8) -> Result<Self, <LogLevel as TryFrom<u8>>::Error> {
		match value {
			0 => Ok(LogLevel::Uknown),
			1 => Ok(LogLevel::Trace),
			2 => Ok(LogLevel::Debug),
			3 => Ok(LogLevel::Info),
			4 => Ok(LogLevel::Warn),
			5 => Ok(LogLevel::Error),
			6 => Ok(LogLevel::Fatal),
			_ => Err("Invalid log level")
		}
	}
}

impl ToString for LogLevel {
	fn to_string(&self) -> String {
		match self {
			LogLevel::Trace => "trace".to_string(),
			LogLevel::Debug => "debug".to_string(),
			LogLevel::Info => "info".to_string(),
			LogLevel::Warn => "warn".to_string(),
			LogLevel::Error => "error".to_string(),
			LogLevel::Fatal => "fatal".to_string(),
			LogLevel::Uknown => "unknown".to_string()
		}
	}
}

impl From<&String> for LogLevel {
	fn from(value: &String) -> Self {
		LogLevel::from_string(value)
	}
}

#[derive(Debug)]
pub enum LogentryDeserializerError {
	InvalidTimestamp,
	InvalidLogLevel,
	InvalidPropKey,
	InvalidPropValue,
	InvalidMessage,
	NotEnoughData
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Prop {
	pub key: String,
	pub value: String
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
	pub version: u16,
	pub random: u32,
	pub timestamp: DateTime<Utc>,
	pub level: LogLevel,
	pub props: Vec<Prop>,
	pub msg: String
}

impl Default for LogEntry {
	fn default() -> Self {
		LogEntry {
			version: 1,
			random: 0,
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![],
			msg: "".to_string()
		}
	}
}

impl LogEntry {
	pub fn id(&self) -> u128 {
		let timestamp_ms = self.timestamp.timestamp_millis() as u128;
		let timestamp_part = timestamp_ms << 32;
		timestamp_part | (self.random as u128)
	}

	pub fn id_string(&self) -> String {
		self.id().to_string()
	}

	pub fn id_hex(&self) -> String {
		format!("{:032x}", self.id())
	}

	pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
		writer.write_u16::<LittleEndian>(self.version)?;
		writer.write_i64::<LittleEndian>(self.timestamp.timestamp_micros())?;
		writer.write_u32::<LittleEndian>(self.random)?;
		writer.write_u8((&self.level).into())?;
		writer.write_u8(self.props.len() as u8)?;
		for prop in &self.props {
			writer.write_u8(prop.key.len() as u8)?;
			writer.write_all(prop.key.as_bytes())?;
			writer.write_u8(prop.value.len() as u8)?;
			writer.write_all(prop.value.as_bytes())?;
		}
		writer.write_u32::<LittleEndian>(self.msg.len() as u32)?;
		writer.write_all(self.msg.as_bytes())?;
		Ok(())
	}

	pub fn fast_deserialize(data: &[u8], ptr: &mut usize) -> Result<LogEntry, LogentryDeserializerError> {
		if *ptr + 16 > data.len() {
			return Err(LogentryDeserializerError::NotEnoughData);
		}
		let version = u16::from_le_bytes(data[*ptr..(*ptr+2)].try_into().unwrap());
		*ptr += 2;
		let timestamp = i64::from_le_bytes(data[*ptr..(*ptr+8)].try_into().unwrap());
		*ptr += 8;
		let timestamp = match DateTime::from_timestamp_micros(timestamp) {
			Some(timestamp) => timestamp,
			None => {
				log::error!("Invalid timestamp");
				return Err(LogentryDeserializerError::InvalidTimestamp);
			}
		};
		let random = u32::from_le_bytes(data[*ptr..(*ptr+4)].try_into().unwrap());
		*ptr += 4;
		let level = match LogLevel::try_from(data[*ptr]) {
			Ok(level) => level,
			Err(_) => return Err(LogentryDeserializerError::InvalidLogLevel)
		};
		*ptr += 1;
		let prop_count = data[*ptr];
		*ptr += 1;
		let mut props = Vec::with_capacity(prop_count as usize);
		for _ in 0..prop_count {
			if *ptr + 1 > data.len() {
				return Err(LogentryDeserializerError::NotEnoughData);
			}
			let key_len = data[*ptr] as usize;
			*ptr += 1;
			if *ptr + key_len + 1 > data.len() {
				return Err(LogentryDeserializerError::NotEnoughData);
			}
			let key = String::from_utf8_lossy(&data[*ptr..*ptr + key_len]).to_string();
			*ptr += key_len;
			if *ptr + 1 > data.len() {
				return Err(LogentryDeserializerError::NotEnoughData);
			}
			let value_len = data[*ptr] as usize;
			*ptr += 1;
			if *ptr + value_len > data.len() {
				return Err(LogentryDeserializerError::NotEnoughData);
			}
			let value = String::from_utf8_lossy(&data[*ptr..*ptr + value_len]).to_string();
			*ptr += value_len;
			props.push(Prop {
				key,
				value
			});
		}
		if *ptr + 4 > data.len() {
			return Err(LogentryDeserializerError::NotEnoughData);
		}
		let msg_len = u32::from_le_bytes(data[*ptr..*ptr + 4].try_into().unwrap()) as usize;
		*ptr += 4;
		if *ptr + msg_len > data.len() {
			return Err(LogentryDeserializerError::NotEnoughData);
		}
		let msg = String::from_utf8_lossy(&data[*ptr..*ptr + msg_len]).to_string();
		*ptr += msg_len;
		Ok(LogEntry {
			version,
			random,
			timestamp,
			level,
			props,
			msg
		})
	}

	pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<LogEntry> {
		let version = reader.read_u16::<LittleEndian>()?;
		let timestamp = reader.read_i64::<LittleEndian>()?;
		let timestamp = match DateTime::from_timestamp_micros(timestamp) {
			Some(timestamp) => timestamp,
			None => {
				log::error!("Invalid timestamp");
				return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid timestamp"));
			}
		};
		let random = reader.read_u32::<LittleEndian>()?;
		let level = reader.read_u8()?;
		let level = match LogLevel::try_from(level) {
			Ok(level) => level,
			Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid log level"))
		};
		let prop_count = reader.read_u8()?;
		let mut props = Vec::with_capacity(prop_count as usize);
		for _ in 0..prop_count {
			let key_len = reader.read_u8()?;
			let mut key = vec![0; key_len as usize];
			reader.read_exact(&mut key)?;
			let key = String::from_utf8_lossy(&key).to_string();
			let value_len = reader.read_u8()?;
			let mut value = vec![0; value_len as usize];
			reader.read_exact(&mut value)?;
			let value = String::from_utf8_lossy(&value).to_string();
			props.push(Prop {
				key,
				value
			});
		}
		let msg_len = reader.read_u32::<LittleEndian>()?;
		let mut msg = vec![0; msg_len as usize];
		reader.read_exact(&mut msg)?;
		let msg = String::from_utf8_lossy(&msg).to_string();
		Ok(LogEntry {
			version,
			random,
			timestamp,
			level,
			props,
			msg
		})
	}
}

pub struct LogEntryChunkParser {
    chunck_parser: ChunkReader,
    pub log_entries: Vec<LogEntry>
}

impl LogEntryChunkParser {
    pub fn new() -> Self {
        Self {
            chunck_parser: ChunkReader::new(),
            log_entries: vec![]
        }
    }

    pub fn add_chunk(&mut self, chunk: Bytes) {
        self.chunck_parser.add_chunk(chunk);
        loop {
            match LogEntry::deserialize(&mut self.chunck_parser) {
                Ok(entry) => {
                    self.chunck_parser.commit();
                    self.log_entries.push(entry);
                },
                Err(_) => {
                    self.chunck_parser.rollback();
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::{LogEntry, LogEntryChunkParser, LogLevel, PuppylogBuilder, Prop};

	#[test]
	fn test_serialize_and_deserialize() {
		use std::io::Cursor;
		use chrono::Utc;
		use super::{LogEntry, LogLevel};

		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![
				Prop { key: "key1".to_string(), value: "value1".to_string() },
				Prop { key: "key2".to_string(), value: "value2".to_string() }
			],
			msg: "Hello, world!".to_string(),
			..Default::default()
		};

		let mut buffer = Cursor::new(vec![]);
		entry.serialize(&mut buffer).unwrap();
		buffer.set_position(0);
		let deserialized = LogEntry::deserialize(&mut buffer).unwrap();

		assert_eq!(entry.timestamp, deserialized.timestamp);
		assert_eq!(entry.level, deserialized.level);
		assert_eq!(entry.props, deserialized.props);
		assert_eq!(entry.msg, deserialized.msg);
	}

	#[test]
	fn parse_from_slice_directly() {
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![
				Prop { key: "key1".to_string(), value: "value1".to_string() },
				Prop { key: "key2".to_string(), value: "value2".to_string() }
			],
			msg: "Hello, world!".to_string(),
			..Default::default()
		};

		let mut buffer = vec![];
		entry.serialize(&mut buffer).unwrap();
		let deserialized = LogEntry::fast_deserialize(&buffer, &mut 0).unwrap();

		assert_eq!(entry.timestamp, deserialized.timestamp);
		assert_eq!(entry.level, deserialized.level);
		assert_eq!(entry.props, deserialized.props);
		assert_eq!(entry.msg, deserialized.msg);
	}

    #[test]
	fn parse_many_log_entries_in_different_chuncks() {
		fn gen_loentries() -> Vec<LogEntry> {
			let mut entries = Vec::new();
			for i in 0..100 {
				entries.push(LogEntry {
					timestamp: chrono::Utc::now(),
					level: LogLevel::Info,
					props: vec![
						Prop { key: "key1".to_string(), value: "value1".to_string() },
						Prop { key: "key2".to_string(), value: "value2".to_string() }
					],
					msg: format!("Hello, world! {}", i),
					..Default::default()
				});
			}
			entries
		}

		let entries = gen_loentries();
		let mut buffer = std::io::Cursor::new(vec![]);
		for entry in &entries {
			entry.serialize(&mut buffer).unwrap();
		}
		buffer.set_position(0);
		let mut reader = LogEntryChunkParser::new();
		let buffer = buffer.into_inner();
		let chuncks = buffer.chunks(buffer.len() / 5).map(|chunk| chunk.to_vec()).collect::<Vec<_>>();

		let mut i = 0;
		for chunck in chuncks {
			reader.add_chunk(chunck.into());
            for entry in &reader.log_entries {
                assert_eq!(entry.timestamp.timestamp_millis(), entries[i].timestamp.timestamp_millis());
                assert_eq!(entry.level, entries[i].level);
                assert_eq!(entry.props, entries[i].props);
                assert_eq!(entry.msg, entries[i].msg);
                i += 1;
            }
            reader.log_entries.clear();
		}
		assert_eq!(i, 100);
	}

	#[test]
	fn parse_one_then_another() {
		PuppylogBuilder::new().stdout().build().unwrap();
		let logentry = LogEntry {
			..Default::default()
		};
		let mut buffer = std::io::Cursor::new(vec![]);
		logentry.serialize(&mut buffer).unwrap();
		let buffer = buffer.into_inner();
		let mut reader = LogEntryChunkParser::new();
		reader.add_chunk(buffer.to_owned().into());
		assert_eq!(reader.log_entries.len(), 1);
		reader.log_entries.clear();
		reader.add_chunk(buffer.to_owned().into());
		assert_eq!(reader.log_entries.len(), 1);
	}
}