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
use chrono::DateTime;
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

	pub async fn find_logs(
		&self,
		query: QueryAst,
		mut cb: impl FnMut(&LogEntry) -> bool,
	) -> anyhow::Result<()> {
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
					return Ok(());
				}
			}
		}
		log::info!("looking from archive");
		let window = chrono::Duration::hours(24);
		let mut prev_end: Option<DateTime<Utc>> = Some(end);
		let mut processed_segments: HashSet<u32> = HashSet::new();

		'outer: loop {
			let end = match self.db.prev_segment_end(prev_end).await? {
				Some(e) => e,
				None => {
					log::info!("no more segments to load");
					break;
				}
			};
			let start = end - window;
			prev_end = Some(start);

			let segments = self
				.db
				.find_segments(&GetSegmentsQuery {
					start: Some(start),
					end: Some(end),
					..Default::default()
				})
				.await
				.unwrap();
			if segments.is_empty() {
				log::info!("no segments found in the range {} - {}", start, end);
				break;
			}
			let segment_ids = segments.iter().map(|s| s.id).collect::<Vec<_>>();
			// let segment_props = self.db.fetch_segments_props(&segment_ids).await.unwrap();
			for segment in &segments {
				if !processed_segments.insert(segment.id) {
					continue;
				}
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
					match check_expr(&query.root, entry, &tz) {
						Ok(true) => {}
						_ => continue,
					}
					if !cb(entry) {
						log::info!("stopped searching logs at {:?}", entry);
						break 'outer;
					}
				}
			}
		}
		Ok(())
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
	use crate::db::NewSegmentArgs;
	use chrono::{Duration, Utc};
	use puppylog::{parse_log_query, LogEntry, LogLevel, Prop};
	use std::fs;
	use std::io::Cursor;
	use tempfile::TempDir;

	// Helper to set up an isolated temp environment & Context for tests.
	async fn prepare_test_ctx() -> (Context, tempfile::TempDir) {
		use std::fs;
		use tempfile::tempdir;

		let dir = tempdir().unwrap();
		let logs_path = dir.path().join("logs");
		fs::create_dir_all(&logs_path).unwrap();

		std::env::set_var("LOG_PATH", &logs_path);
		let db_path = dir.path().join("db.sqlite");
		if db_path.exists() {
			fs::remove_file(&db_path).unwrap();
		}
		std::env::set_var("DB_PATH", &db_path);
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));

		let ctx = Context::new().await;
		(ctx, dir)
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn find_logs_from_memory() {
		let (ctx, dir) = prepare_test_ctx().await;

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
		.await
		.unwrap();

		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "match me");
		drop(dir);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn find_logs_from_segment() {
		let (ctx, dir) = prepare_test_ctx().await;

		let entry = LogEntry {
			timestamp: Utc::now() - Duration::hours(47),
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
		query.end_date = Some(Utc::now());
		ctx.find_logs(query, |log| {
			found.push(log.clone());
			true
		})
		.await
		.unwrap();

		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "segment log");
		drop(dir);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn find_logs_not_found() {
		let (ctx, dir) = prepare_test_ctx().await;

		// Search for a message that does not exist anywhere.
		let query = parse_log_query("msg = \"non‑existent log\"").unwrap();
		let mut found = Vec::<LogEntry>::new();
		ctx.find_logs(query, |log| {
			found.push(log.clone());
			true
		})
		.await
		.unwrap();

		// We expect to find **no** matching entries.
		assert!(found.is_empty(), "unexpectedly found logs: {:?}", found);
		drop(dir);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn find_logs_skip_duplicate_segments() {
		let (ctx, dir) = prepare_test_ctx().await;

		let now = Utc::now();

		// Older segment spanning a wide window so it overlaps multiple queries
		let entry_old = LogEntry {
			timestamp: now - Duration::hours(40),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "service".to_string(),
				value: "old".to_string(),
			}],
			msg: "duplicate".to_string(),
			..Default::default()
		};
		let mut old_seg = LogSegment::new();
		old_seg.add_log_entry(entry_old.clone());
		old_seg.sort();
		let mut buff = Vec::new();
		old_seg.serialize(&mut buff);
		let orig_size = buff.len();
		let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
		let comp_size = compressed.len();
		let old_seg_id = ctx
			.db
			.new_segment(NewSegmentArgs {
				first_timestamp: entry_old.timestamp - Duration::hours(10),
				last_timestamp: entry_old.timestamp + Duration::hours(10),
				original_size: orig_size,
				compressed_size: comp_size,
				logs_count: 1,
			})
			.await
			.unwrap();
		ctx.db
			.upsert_segment_props(old_seg_id, entry_old.props.iter())
			.await
			.unwrap();
		let path = log_path().join(format!("{}.log", old_seg_id));
		fs::write(path, compressed).unwrap();

		// Newer segment to trigger overlapping search window
		let entry_new = LogEntry {
			timestamp: now - Duration::hours(5),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "service".to_string(),
				value: "new".to_string(),
			}],
			msg: "new".to_string(),
			..Default::default()
		};
		let mut new_seg = LogSegment::new();
		new_seg.add_log_entry(entry_new.clone());
		new_seg.sort();
		let mut buff = Vec::new();
		new_seg.serialize(&mut buff);
		let orig_size = buff.len();
		let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
		let comp_size = compressed.len();
		let new_seg_id = ctx
			.db
			.new_segment(NewSegmentArgs {
				first_timestamp: entry_new.timestamp,
				last_timestamp: entry_new.timestamp,
				original_size: orig_size,
				compressed_size: comp_size,
				logs_count: 1,
			})
			.await
			.unwrap();
		ctx.db
			.upsert_segment_props(new_seg_id, entry_new.props.iter())
			.await
			.unwrap();
		let path = log_path().join(format!("{}.log", new_seg_id));
		fs::write(path, compressed).unwrap();

		let mut query = parse_log_query("msg = \"duplicate\"").unwrap();
		query.end_date = Some(now);
		let mut found = Vec::new();
		ctx.find_logs(query, |log| {
			found.push(log.clone());
			true
		})
		.await
		.unwrap();

		assert_eq!(found.len(), 1, "log returned more than once");
		assert_eq!(found[0].msg, "duplicate");
		drop(dir);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn skips_segments_newer_than_in_memory() {
		use chrono::{Duration, Utc};
		use puppylog::{parse_log_query, LogEntry, LogLevel, Prop};
		use std::fs;
		use std::io::Cursor;
		use zstd;

		let (ctx, dir) = prepare_test_ctx().await;
		let now = Utc::now();

		let mem_entry = LogEntry {
			timestamp: now - Duration::hours(2),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "svc".into(),
				value: "mem".into(),
			}],
			msg: "harmless".into(),
			..Default::default()
		};
		ctx.save_logs(&[mem_entry.clone()]).await;

		let skip_entry = LogEntry {
			timestamp: mem_entry.timestamp + Duration::hours(1),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "svc".into(),
				value: "skip".into(),
			}],
			msg: "should_not_be_seen".into(),
			..Default::default()
		};
		let new_seg_id = {
			let mut seg = LogSegment::new();
			seg.add_log_entry(skip_entry.clone());
			seg.sort();
			let mut buff = Vec::new();
			seg.serialize(&mut buff);
			let original_size = buff.len();
			let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
			let compressed_size = compressed.len();

			let id = ctx
				.db
				.new_segment(NewSegmentArgs {
					first_timestamp: skip_entry.timestamp,
					last_timestamp: skip_entry.timestamp,
					original_size,
					compressed_size,
					logs_count: 1,
				})
				.await
				.unwrap();

			ctx.db
				.upsert_segment_props(id, skip_entry.props.iter())
				.await
				.unwrap();
			fs::write(log_path().join(format!("{}.log", id)), compressed).unwrap();
			id
		};

		let want_entry = LogEntry {
			timestamp: mem_entry.timestamp - Duration::hours(5),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "svc".into(),
				value: "want".into(),
			}],
			msg: "target".into(),
			..Default::default()
		};
		{
			let mut seg = LogSegment::new();
			seg.add_log_entry(want_entry.clone());
			seg.sort();
			let mut buff = Vec::new();
			seg.serialize(&mut buff);
			let original_size = buff.len();
			let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
			let compressed_size = compressed.len();

			let id = ctx
				.db
				.new_segment(NewSegmentArgs {
					first_timestamp: want_entry.timestamp,
					last_timestamp: want_entry.timestamp,
					original_size,
					compressed_size,
					logs_count: 1,
				})
				.await
				.unwrap();
			ctx.db
				.upsert_segment_props(id, want_entry.props.iter())
				.await
				.unwrap();
			fs::write(log_path().join(format!("{}.log", id)), compressed).unwrap();
		}

		let q_skip = parse_log_query("msg = \"should_not_be_seen\"").unwrap();
		let mut seen = Vec::<LogEntry>::new();
		ctx.find_logs(q_skip, |e| {
			seen.push(e.clone());
			true
		})
		.await
		.unwrap();
		assert!(
			seen.is_empty(),
			"search incorrectly visited segment {new_seg_id} that is newer than in-memory logs"
		);

		let q_target = parse_log_query("msg = \"target\"").unwrap();
		let mut found = Vec::<LogEntry>::new();
		ctx.find_logs(q_target, |e| {
			found.push(e.clone());
			true
		})
		.await
		.unwrap();
		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "target");
		drop(dir);
	}
}
