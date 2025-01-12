use std::collections::HashMap;
use std::path::PathBuf;
use chrono::{Datelike, Utc};
use puppylog::{LogEntry, LogEntryParser};
use tokio::{fs::{File, OpenOptions}, io::{AsyncReadExt, AsyncWriteExt}};
use crate::config::log_path;
use crate::types::LogsQuery;

pub struct Storage {
	files: HashMap<String, File>,
	logspath: PathBuf,
	buff: Vec<u8>
}

impl Storage {
	pub fn new() -> Self {
		Storage {
			files: HashMap::new(),
			logspath: log_path(),
			buff: Vec::new()
		}
	}

	pub async fn save_log_entry(&mut self, log_entry: &LogEntry) -> anyhow::Result<()> {
		let folder = self.logspath.join(format!("{}/{}/{}", log_entry.timestamp.year(), log_entry.timestamp.month(), log_entry.timestamp.day()));
		if !folder.exists() {
			tokio::fs::create_dir_all(&folder).await?;
		}
		let path = folder.join(format!("{}-{}-{}.log", log_entry.timestamp.year(), log_entry.timestamp.month(), log_entry.timestamp.day()));
		let file = match self.files.get_mut(&path.to_string_lossy().to_string()) {
			Some(file) => file,
			None => {
				let file = if path.exists() {
					OpenOptions::new().append(true).open(&path).await?
				} else {
					File::create(&path).await?
				};
				self.files.entry(path.to_string_lossy().to_string()).or_insert(file)
			}
		};
		log_entry.serialize(&mut self.buff)?;
		file.write_all(&self.buff).await?;
		self.buff.clear();
		Ok(())
	}
}

pub async fn search_logs(query: LogsQuery) -> anyhow::Result<Vec<LogEntry>> {
	let logspath = log_path();
	let start = query.start.unwrap_or(Utc::now());
	let count = query.count.unwrap_or(50);
	let mut logs: Vec<LogEntry> = Vec::new();
	let mut d = start;
	let mut parser = LogEntryParser::new();
	while logs.len() < count {
		let path = logspath.join(format!("{}/{}/{}/{}-{}-{}.log", d.year(), d.month(), d.day(), d.year(), d.month(), d.day()));
			if path.exists() {
				let file = OpenOptions::new().read(true).open(&path).await?;
				let mut reader = tokio::io::BufReader::new(file);
				let mut buffer = [0; 4096];
				loop {
					let n = reader.read(&mut buffer).await?;
					if n == 0 {
						break;
					}
					parser.parse(&buffer[..n]);
					for entry in parser.log_entries.drain(..) {
						if query.matches(&entry) {
							logs.push(entry);
						}
					}
				}
			} else {
				break;
			}
		d = d + chrono::Duration::days(1);
	}

	Ok(logs)
}