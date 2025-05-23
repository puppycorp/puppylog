use crate::config::log_path;
use chrono::Datelike;
use puppylog::check_expr;
use puppylog::LogEntry;
use puppylog::LogentryDeserializerError;
use puppylog::QueryAst;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use tokio::fs::read_dir;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

struct Storage {
	files: HashMap<String, File>,
	logspath: PathBuf,
	buff: Vec<u8>,
}

impl Storage {
	pub fn new() -> Self {
		Storage {
			files: HashMap::new(),
			logspath: log_path(),
			buff: Vec::new(),
		}
	}

	pub async fn save_log_entry(&mut self, log_entry: LogEntry) -> anyhow::Result<()> {
		let folder = self.logspath.join(format!(
			"{}/{}/{}",
			log_entry.timestamp.year(),
			log_entry.timestamp.month(),
			log_entry.timestamp.day()
		));
		if !folder.exists() {
			tokio::fs::create_dir_all(&folder).await?;
		}
		let path = folder.join(format!(
			"{}-{}-{}.log",
			log_entry.timestamp.year(),
			log_entry.timestamp.month(),
			log_entry.timestamp.day()
		));
		let file = match self.files.get_mut(&path.to_string_lossy().to_string()) {
			Some(file) => file,
			None => {
				let file = if path.exists() {
					OpenOptions::new().append(true).open(&path).await?
				} else {
					File::create(&path).await?
				};
				self.files
					.entry(path.to_string_lossy().to_string())
					.or_insert(file)
			}
		};
		log_entry.serialize(&mut self.buff)?;
		file.write_all(&self.buff).await?;
		self.buff.clear();
		Ok(())
	}
}

async fn worker(mut rx: mpsc::Receiver<LogEntry>) {
	let mut storage = Storage::new();
	while let Some(log_entry) = rx.recv().await {
		if let Err(err) = storage.save_log_entry(log_entry).await {
			log::error!("Failed to save log entry: {}", err);
		}
	}
}

#[derive(Debug, Clone)]
pub struct LogEntrySaver {
	tx: mpsc::Sender<LogEntry>,
}

impl LogEntrySaver {
	pub fn new() -> Self {
		let (tx, rx) = mpsc::channel(100);
		tokio::spawn(async move {
			worker(rx).await;
		});
		LogEntrySaver { tx }
	}

	pub async fn save(&self, log_entry: LogEntry) -> anyhow::Result<()> {
		self.tx.send(log_entry).await?;
		Ok(())
	}
}

async fn get_years() -> Vec<u32> {
	let logs_path = log_path();
	let mut years = read_dir(logs_path).await.unwrap();
	let mut years_vec = Vec::new();
	while let Some(year) = years.next_entry().await.unwrap() {
		if year.file_type().await.unwrap().is_dir() {
			if let Ok(year) = year.file_name().into_string().unwrap().parse::<u32>() {
				years_vec.push(year);
			}
		}
	}
	years_vec
}

async fn get_months(year: u32) -> Vec<u32> {
	let logs_path = log_path();
	let mut months = read_dir(logs_path.join(year.to_string())).await.unwrap();
	let mut months_vec = Vec::new();
	while let Some(month) = months.next_entry().await.unwrap() {
		if let Ok(month) = month.file_name().into_string().unwrap().parse::<u32>() {
			months_vec.push(month);
		}
	}
	months_vec
}

async fn get_days(year: u32, month: u32) -> Vec<u32> {
	let logs_path = log_path();
	let mut days = read_dir(logs_path.join(year.to_string()).join(month.to_string()))
		.await
		.unwrap();
	let mut days_vec = Vec::new();
	while let Some(day) = days.next_entry().await.unwrap() {
		if let Ok(day) = day.file_name().into_string().unwrap().parse::<u32>() {
			days_vec.push(day);
		}
	}
	days_vec
}

pub async fn search_logs(query: QueryAst) -> anyhow::Result<Vec<LogEntry>> {
	let logspath = log_path();
	if !logspath.exists() {
		tokio::fs::create_dir_all(&logspath).await?;
	}
	let offset = query.offset.unwrap_or(0);
	let count = query.limit.unwrap_or(200);
	let mut logs: Vec<LogEntry> = Vec::new();
	let mut years = get_years().await;
	years.sort_by(|a, b| b.cmp(a));
	'main: for year in years {
		let mut months = get_months(year).await;
		months.sort_by(|a, b| b.cmp(a));
		for month in months {
			let mut days = get_days(year, month).await;
			days.sort_by(|a, b| b.cmp(a));
			for day in days {
				log::info!("Searching logs for {}/{}/{}", year, month, day);
				let path = logspath.join(format!(
					"{}/{}/{}/{}-{}-{}.log",
					year, month, day, year, month, day
				));
				if path.exists() {
					let timer = Instant::now();
					let file = OpenOptions::new().read(true).open(&path).await?;
					let mut reader = tokio::io::BufReader::new(file);
					let mut buffer = Vec::new();
					reader.read_to_end(&mut buffer).await?;
					log::info!("Read {} bytes in {:?}", buffer.len(), timer.elapsed());
					let timer = Instant::now();
					let mut total_loglines = 0;
					let mut total_expr_time = 0;
					let mut ptr = 0;
					loop {
						match LogEntry::fast_deserialize(&buffer, &mut ptr) {
							Ok(log_entry) => {
								total_loglines += 1;
								let timer = Instant::now();
								if check_expr(&query.root, &log_entry).unwrap() {
									logs.push(log_entry);
								}
								total_expr_time += timer.elapsed().as_nanos();
							}
							Err(LogentryDeserializerError::NotEnoughData) => {
								break;
							}
							Err(err) => {
								log::error!("Error deserializing log entry: {:?}", err);
							}
						}
					}
					log::info!(
						"Parsed {} loglines in {:?} expr time {}",
						total_loglines,
						timer.elapsed(),
						total_expr_time / 1000000
					);
					let timer = Instant::now();
					logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
					log::info!("sorting {} logs in {:?}", logs.len(), timer.elapsed());
					let total_len = offset + count;
					if logs.len() >= total_len {
						break 'main;
					}
				}
			}
		}
	}

	log::info!("Found {} logs", logs.len());

	Ok(logs.into_iter().skip(offset).take(count).collect())
}
