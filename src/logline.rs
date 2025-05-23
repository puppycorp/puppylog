use std::fs::File;
use std::io::Read;
use std::path::Path;

use puppylog::LogEntry;
use puppylog::LogentryDeserializerError;

pub fn load_log_entries<P: AsRef<Path>>(path: P, logs: &mut Vec<LogEntry>) {
	let mut file = File::open(path).unwrap();
	let mut buff = Vec::new();
	file.read_to_end(&mut buff).unwrap();
	let mut ptr = 0;
	loop {
		match LogEntry::fast_deserialize(&buff, &mut ptr) {
			Ok(log) => logs.push(log),
			Err(LogentryDeserializerError::NotEnoughData) => {
				break;
			}
			Err(e) => {
				eprintln!("Error deserializing log entry: {:?}", e);
				continue;
			}
		};
	}
}
