use std::fs::OpenOptions;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use puppylog::LogEntry;
use crate::config::log_path;

fn wal_path() -> PathBuf {
	log_path().join("wal.log")
}

enum Cmd {
	WriteLog(LogEntry),
	Clear
}

#[derive(Debug)]
pub struct Wal {
	tx: mpsc::Sender<Cmd>
}

impl Wal {
	pub fn new() -> Self {
		let (tx, rx) = mpsc::channel();
		thread::spawn(move || {
			let path = wal_path();
			log::info!("trying to open wal file: {:?}", path);
			let mut wal_file = match OpenOptions::new()
				.append(true)
				.create(true)
				.open(path) {
				Ok(file) => file,
				Err(err) => {
					log::error!("Failed to open wal file: {}", err);
					return;
				}
			};
			while let Ok(cmd) = rx.recv() {
				match cmd {
					Cmd::WriteLog(log) => {
						log.serialize(&mut wal_file).unwrap();
					},
					Cmd::Clear => {
						log::info!("clearing logs from wal");
						wal_file.set_len(0).unwrap();
					}
				}
			}
		});
		Self {
			tx
		}
	}

	pub fn write(&self, log: LogEntry) {
		if let Err(err) = self.tx.send(Cmd::WriteLog(log)) {
			log::error!("Failed to write to wal: {}", err);
		}
	}

	pub fn clear(&self) {
		if let Err(err) = self.tx.send(Cmd::Clear) {
			log::error!("Failed to clear wal: {}", err);
		}
	}
}

pub fn load_logs_from_wal() -> Vec<LogEntry> {
	let timer = std::time::Instant::now();
	let path = wal_path();
	if !path.exists() {
		return Vec::new();
	}
	let mut logs = Vec::new();
	let mut file = OpenOptions::new().read(true).open(path).unwrap();
	let mut buff = Vec::new();
	file.read_to_end(&mut buff).unwrap();
	let mut ptr = 0;
	loop {
		match LogEntry::fast_deserialize(&buff, &mut ptr) {
			Ok(log) => logs.push(log),
			Err(puppylog::LogentryDeserializerError::NotEnoughData) => {
				break;
			},
			Err(e) => {
				log::error!("Error deserializing log entry: {:?}", e);
				continue;
			}
		};
	}
	log::info!("Loaded {} logs from wal in {:?}", logs.len(), timer.elapsed());
	logs
}