use core::time;
use std::{fs::{File, OpenOptions}, io::{self, Error, Read, Write}, path::Path};
use bytes::Bytes;
use chrono::{DateTime, Timelike, Utc};
use futures::Stream;
use futures_util::StreamExt;
use puppylog::LogEntry;

const LOG_FILE_MAGIC: &str = "PUPPYLOG";
const LOG_FILE_VERSION: u16 = 1;
const LOG_FILE_HEADER_SIZE: usize = 16;

pub struct Logfile {
	file: File,
}

impl Logfile {
	pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
		let mut file: File = OpenOptions::new().read(true).write(true).append(true).open(path)?;
		let mut header_data = [0_u8; LOG_FILE_HEADER_SIZE];
		file.read(&mut header_data)?;
		if &header_data[0..8] != LOG_FILE_MAGIC.as_bytes() {
			return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid log file"));
		}

		Ok(Logfile {
			file
		})
	}

	pub fn save_log_entry(&mut self, entry: &LogEntry) -> Result<(), Error> {
		// let timestamp = entry.timestamp.format("%Y-%m-%dT%H:%M:%S%.6f").to_string();
		// let logline = format!("{} {} {}\n", timestamp, entry.level, entry.message);
		// self.file.write_all(logline.as_bytes())?;
		Ok(())
	}
}