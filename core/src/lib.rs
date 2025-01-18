mod logfile;
mod circle_buffer;
mod chunk_reader;

use std::io;
use std::io::Read;
use std::io::Write;
use byteorder::LittleEndian;
use chrono::DateTime;
use chrono::Utc;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use serde::Deserialize;
use serde::Serialize;
pub use circle_buffer::CircularBuffer;
pub use chunk_reader::ChunckReader;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LogLevel {
	Debug,
	Info,
	Warn,
	Error,
	Uknown
}

impl LogLevel {
	pub fn from_string(value: &str) -> Self {
		match value {
			"debug" => LogLevel::Debug,
			"info" => LogLevel::Info,
			"warn" => LogLevel::Warn,
			"error" => LogLevel::Error,
			"INFO" => LogLevel::Info,
			"DEBUG" => LogLevel::Debug,
			"WARN" => LogLevel::Warn,
			"ERROR" => LogLevel::Error,
			_ => LogLevel::Uknown
		}
	}

	pub fn from_i64(value: i64) -> Self {
		match value {
			0 => LogLevel::Debug,
			1 => LogLevel::Info,
			2 => LogLevel::Warn,
			3 => LogLevel::Error,
			_ => LogLevel::Uknown
		}
	}
}

impl Into<u8> for &LogLevel {
	fn into(self) -> u8 {
		match self {
			LogLevel::Debug => 0,
			LogLevel::Info => 1,
			LogLevel::Warn => 2,
			LogLevel::Error => 3,
			LogLevel::Uknown => 4
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

impl ToString for LogLevel {
	fn to_string(&self) -> String {
		match self {
			LogLevel::Debug => "debug".to_string(),
			LogLevel::Info => "info".to_string(),
			LogLevel::Warn => "warn".to_string(),
			LogLevel::Error => "error".to_string(),
			LogLevel::Uknown => "unknown".to_string()
		}
	}
}

impl From<&String> for LogLevel {
	fn from(value: &String) -> Self {
		LogLevel::from_string(value)
	}
}


// impl TryFrom<&String> for LogLevel {
// 	type Error = &'static str;

// 	fn try_from(value: &String) -> Result<Self, Self::Error> {
// 		match value.as_str() {
// 			"debug" => Ok(LogLevel::Debug),
// 			"info" => Ok(LogLevel::Info),
// 			"warn" => Ok(LogLevel::Warn),
// 			"error" => Ok(LogLevel::Error),
// 			"DEBUG" => Ok(LogLevel::Debug),
// 			"INFO" => Ok(LogLevel::Info),
// 			"WARN" => Ok(LogLevel::Warn),
// 			"ERROR" => Ok(LogLevel::Error),
// 			_ => Err("Invalid log level")
// 		}
// 	}
// }

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
		let msg = String::from_utf8_lossy(&msg).to_string();
		Ok(LogEntry {
			timestamp,
			level,
			props,
			msg
		})
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

}