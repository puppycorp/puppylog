use std::io::Cursor;
use std::{collections::HashMap, time::Instant};
use std::path::PathBuf;
use chrono::{Datelike, Utc};
use puppylog::{LogEntry, LogEntryParser};
use tokio::{fs::{read_dir, File, OpenOptions}, io::{AsyncReadExt, AsyncWriteExt}};
use crate::config::log_path;
use crate::log_query::{Expr, Operator, QueryAst, Value};
use crate::query_eval::check_expr;
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
	let mut days = read_dir(logs_path.join(year.to_string()).join(month.to_string())).await.unwrap();
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
	// let start = query.start.unwrap_or(Utc::now());
	// let count = query.count.unwrap_or(50);
	let offset = query.offset.unwrap_or(0);
	let count = query.limit.unwrap_or(200);
	let mut logs: Vec<LogEntry> = Vec::new();
	let mut parser = LogEntryParser::new();
	let mut years = get_years().await;
	years.sort_by(|a, b| b.cmp(a));
	'main: for year in years {
		// if year < start.year() as u32 {
		// 	break;
		// }
		let mut months = get_months(year).await;
		months.sort_by(|a, b| b.cmp(a));
		for month in months {
			// if year == start.year() as u32 && month < start.month() as u32 {
			// 	break;
			// }
			let mut days = get_days(year, month).await;
			days.sort_by(|a, b| b.cmp(a));
			for day in days {
				// if year == start.year() as u32 && month == start.month() as u32 && day < start.day() as u32 {
				// 	break;
				// }
				log::info!("Searching logs for {}/{}/{}", year, month, day);
				let path = logspath.join(format!("{}/{}/{}/{}-{}-{}.log", year, month, day, year, month, day));
				if path.exists() {
					let timer = Instant::now();
					let file = OpenOptions::new().read(true).open(&path).await?;
					let mut reader = tokio::io::BufReader::new(file);
					let mut buffer = Vec::new();
					reader.read_to_end(&mut buffer).await?;
					log::info!("Read {} bytes in {:?}", buffer.len(), timer.elapsed());
					let mut buffer = Cursor::new(buffer);
					let timer = Instant::now();
					let mut total_loglines = 0;
					while let Ok(log_entry) = LogEntry::deserialize(&mut buffer) {
						total_loglines += 1;
						if check_expr(&query.root, &log_entry).unwrap() {
							logs.push(log_entry);
						}
					}
					log::info!("Parsed {} loglines in {:?}", total_loglines, timer.elapsed());
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