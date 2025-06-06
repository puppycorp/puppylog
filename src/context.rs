use crate::config::log_path;
use crate::db::open_db;
use crate::db::NewSegmentArgs;
use crate::db::DB;
use crate::segment::LogSegment;
use crate::settings::Settings;
use crate::subscribe_worker::Subscriber;
use crate::subscribe_worker::Worker;
use crate::types::GetSegmentsQuery;
use crate::upload_guard::UploadGuard;
use crate::wal::load_logs_from_wal;
use crate::wal::Wal;
use chrono::Utc;
use puppylog::check_expr;
use puppylog::LogEntry;
use puppylog::PuppylogEvent;
use puppylog::QueryAst;
use std::collections::HashSet;
use std::fs::File;
use std::io::Cursor;
use std::io::Write;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

const CONCURRENCY_LIMIT: usize = 10;

#[derive(Debug)]
pub struct Context {
	pub subscriber: Subscriber,
	pub publisher: Sender<LogEntry>,
	pub settings: Settings,
	pub event_tx: broadcast::Sender<PuppylogEvent>,
	pub db: DB,
	pub current: Mutex<LogSegment>,
	pub wal: Wal,
	pub upload_queue: AtomicUsize,
}

impl Context {
	pub async fn new() -> Self {
		let (subtx, subrx) = mpsc::channel(100);
		let (pubtx, pubrx) = mpsc::channel(100);
		tokio::spawn(async move {
			Worker::new(subrx, pubrx).run().await;
		});
		let (event_tx, _) = broadcast::channel(100);
		let wal = Wal::new();
		let logs = load_logs_from_wal();
		let db = DB::new(open_db());
		Context {
			subscriber: Subscriber::new(subtx),
			publisher: pubtx,
			settings: Settings::load().unwrap(),
			event_tx,
			db,
			current: Mutex::new(LogSegment::with_logs(logs)),
			wal,
			upload_queue: AtomicUsize::new(0),
		}
	}

	pub async fn save_logs(&self, logs: &[LogEntry]) {
		let mut current = self.current.lock().await;
		current.buffer.extend_from_slice(logs);
		for entry in logs {
			self.wal.write(entry.clone());
		}
		current.sort();
		if current.buffer.len() > 50_000 {
			log::info!("flushing segment with {} logs", current.buffer.len());
			self.wal.clear();
			let first_timestamp = current.buffer.first().unwrap().timestamp;
			let last_timestamp = current.buffer.last().unwrap().timestamp;
			let mut buff = Cursor::new(Vec::new());
			current.serialize(&mut buff);
			let original_size = buff.position() as usize;
			buff.set_position(0);
			let buff = zstd::encode_all(buff, 0).unwrap();
			let compressed_size = buff.len();
			let segment_id = self
				.db
				.new_segment(NewSegmentArgs {
					first_timestamp,
					last_timestamp,
					logs_count: current.buffer.len() as u64,
					original_size,
					compressed_size,
				})
				.await
				.unwrap();
			let mut unique_props = HashSet::new();
			for log in &current.buffer {
				for prop in &log.props {
					unique_props.insert(prop.clone());
				}
			}
			self.db
				.upsert_segment_props(segment_id, unique_props.iter())
				.await
				.unwrap();
			let path = log_path().join(format!("{}.log", segment_id));
			let mut file = File::create(&path).unwrap();
			file.write_all(&buff).unwrap();
			current.buffer.clear();
		}
	}

	pub async fn find_logs(&self, query: QueryAst, mut cb: impl FnMut(&LogEntry) -> bool) {
		let mut end = query.end_date.unwrap_or(Utc::now());
		let tz = query
			.tz_offset
			.unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
		{
			let current = self.current.lock().await;
			let iter = current.iter();
			for entry in iter {
				if entry.timestamp > end {
					continue;
				}
				end = entry.timestamp;
				match check_expr(&query.root, entry, &tz) {
					Ok(true) => {}
					_ => continue,
				}
				if !cb(entry) {
					return;
				}
			}
		}
		log::info!("looking from archive");
		loop {
			let segments = self
				.db
				.find_segments(&GetSegmentsQuery {
					end: Some(end),
					count: Some(100),
					..Default::default()
				})
				.await
				.unwrap();
			if segments.is_empty() {
				log::info!("no more segments to load");
				break;
			}
			let segment_ids = segments.iter().map(|s| s.id).collect::<Vec<_>>();
			// let segment_props = self.db.fetch_segments_props(&segment_ids).await.unwrap();
			for segment in &segments {
				// let props = match segment_props.get(&segment.id) {
				// 	Some(props) => props,
				// 	None => continue,
				// };
				// let check = check_props(&query.root, &props).unwrap_or_default();
				// if !check {
				// 	end = segment.first_timestamp;
				// 	continue;
				// }
				let path = log_path().join(format!("{}.log", segment.id));
				log::info!("loading segment from disk: {}", path.display());
				let file: File = File::open(path).unwrap();
				let mut decoder = zstd::Decoder::new(file).unwrap();
				let segment = LogSegment::parse(&mut decoder);
				let iter = segment.iter();
				for entry in iter {
					if end < entry.timestamp {
						continue;
					}
					end = entry.timestamp;
					match check_expr(&query.root, entry, &tz) {
						Ok(true) => {}
						_ => continue,
					}
					if !cb(entry) {
						return;
					}
				}
			}
		}
	}

	pub fn allowed_to_upload(&self) -> bool {
		self.upload_queue.load(Ordering::SeqCst) < CONCURRENCY_LIMIT
	}

	pub fn upload_guard(&self) -> Result<UploadGuard<'_>, &str> {
		UploadGuard::new(&self.upload_queue, CONCURRENCY_LIMIT)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use puppylog::{parse_log_query, LogEntry, LogLevel, Prop};
	use std::fs;
	use std::io::Cursor;
	use tempfile::tempdir;

	#[tokio::test]
	#[serial_test::serial]
	async fn find_logs_from_memory() {
		let dir = tempdir().unwrap();
		std::env::set_var("LOG_PATH", dir.path().join("logs"));
		std::env::set_var("DB_PATH", dir.path().join("db.sqlite"));
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));
		fs::create_dir_all(dir.path().join("logs")).unwrap();

		let ctx = Context::new().await;

		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "service".to_string(),
				value: "search".to_string(),
			}],
			msg: "match me".to_string(),
			..Default::default()
		};

		ctx.save_logs(&[entry.clone()]).await;

		let query = parse_log_query("msg = \"match me\"").unwrap();
		let mut found = Vec::new();
		ctx.find_logs(query, |log| {
			found.push(log.clone());
			true
		})
		.await;

		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "match me");
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn find_logs_from_segment() {
		let dir = tempdir().unwrap();
		std::env::set_var("LOG_PATH", dir.path().join("logs"));
		std::env::set_var("DB_PATH", dir.path().join("db.sqlite"));
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));
		fs::create_dir_all(dir.path().join("logs")).unwrap();

		let ctx = Context::new().await;

		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "service".to_string(),
				value: "segment".to_string(),
			}],
			msg: "segment log".to_string(),
			..Default::default()
		};

		let mut segment = LogSegment::new();
		segment.add_log_entry(entry.clone());
		segment.sort();
		let mut buff = Vec::new();
		segment.serialize(&mut buff);
		let original_size = buff.len();
		let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
		let compressed_size = compressed.len();

		let segment_id = ctx
			.db
			.new_segment(NewSegmentArgs {
				first_timestamp: entry.timestamp,
				last_timestamp: entry.timestamp,
				original_size,
				compressed_size,
				logs_count: 1,
			})
			.await
			.unwrap();

		ctx.db
			.upsert_segment_props(segment_id, entry.props.iter())
			.await
			.unwrap();

		let path = log_path().join(format!("{}.log", segment_id));
		fs::write(path, compressed).unwrap();

		let mut found = Vec::new();
		let mut query = parse_log_query("msg = \"segment log\"").unwrap();
		query.end_date = Some(Utc::now() + chrono::Duration::seconds(1));
		ctx.find_logs(query, |log| {
			found.push(log.clone());
			true
		})
		.await;

		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "segment log");
	}
}
