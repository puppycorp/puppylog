use std::collections::HashMap;
use std::path::PathBuf;
use chrono::Datelike;
use puppylog::LogEntry;
use tokio::{fs::{File, OpenOptions}, io::AsyncWriteExt};
use crate::config::log_path;

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