use core::time;
use std::io::{self, Read, Write};
use byteorder::{BigEndian, LittleEndian, WriteBytesExt};
use byteorder::ReadBytesExt;
use chrono::{DateTime, Utc};

#[derive(Debug, serde::Serialize)]
pub struct Logline {
    timestamp: String,
    loglevel: String,
    message: String
}

// 2025-01-06T21:16:54.279466 INFO device SensorY disconnected
pub fn parse_logline(logline: &str) -> Logline {
    let mut parts = logline.split(" ");
    let timestamp = parts.next().unwrap();
    let loglevel = parts.next().unwrap();
    Logline {
        timestamp: timestamp.to_string(),
        loglevel: loglevel.to_string(),
        message: parts.collect::<Vec<&str>>().join(" ")
    }
}

pub enum LogLevel {
	Debug,
	Info,
	Warn,
	Error,
	Fatal
}

impl Into<u8> for &LogLevel {
	fn into(self) -> u8 {
		match self {
			LogLevel::Debug => 0,
			LogLevel::Info => 1,
			LogLevel::Warn => 2,
			LogLevel::Error => 3,
			LogLevel::Fatal => 4
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
			4 => LogLevel::Fatal,
			_ => panic!("Invalid log level")
		}
	}
}

pub struct LogEntry {
	pub timestamp: DateTime<Utc>,
	pub level: LogLevel,
	pub props: Vec<(String, String)>,
	pub msg: String
}

impl LogEntry {
	pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
		writer.write_i64::<LittleEndian>(self.timestamp.timestamp())?;
		writer.write_u8((&self.level).into())?;
		writer.write_u8(self.props.len() as u8)?;


		Ok(())
	}

	pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<LogEntry> {
		let timestamp = reader.read_i64::<BigEndian>()?;
		let timestamp = DateTime::from_timestamp_millis(timestamp).unwrap();
		let level = reader.read_u8()?;
		let level = LogLevel::from(level);
		let prop_count = reader.read_u8()?;
		let mut props = vec![];
		for _ in 0..prop_count {
			let key_len = reader.read_u8()?;
			let mut key = vec![0; key_len as usize];
			reader.read_exact(&mut key)?;
			let key = String::from_utf8(key).unwrap();
			let value_len = reader.read_u8()?;
			let mut value = vec![0; value_len as usize];
			reader.read_exact(&mut value)?;
			let value = String::from_utf8(value).unwrap();
			props.push((key, value));
		}

		let msg_len = reader.read_u16::<BigEndian>()?;
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