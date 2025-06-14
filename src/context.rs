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
use puppylog::match_date_range;
use puppylog::LogEntry;
use puppylog::Prop;
use puppylog::PuppylogEvent;
use puppylog::QueryAst;
use puppylog::{check_expr, check_props, extract_device_ids, timestamp_bounds};
use std::collections::HashSet;
use std::fs::File;
use std::io::Cursor;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
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
	logs_path: PathBuf,
}

impl Context {
	pub async fn new<P: AsRef<Path>>(logs_path: P) -> Self {
		let (subtx, subrx) = mpsc::channel(100);
		let (pubtx, pubrx) = mpsc::channel(100);
		tokio::spawn(async move {
			Worker::new(subrx, pubrx).run().await;
		});
		let (event_tx, _) = broadcast::channel(100);
		let wal = Wal::new();
		let logs = if cfg!(test) {
			Vec::new()
		} else {
			load_logs_from_wal()
		};
		let db = DB::new(open_db());
		let settings = if cfg!(test) {
			Settings::new()
		} else {
			Settings::load().unwrap_or_else(|_| Settings::new())
		};
		Context {
			subscriber: Subscriber::new(subtx),
			publisher: pubtx,
			settings,
			event_tx,
			db,
			current: Mutex::new(LogSegment::with_logs(logs)),
			wal,
			upload_queue: AtomicUsize::new(0),
			logs_path: logs_path.as_ref().to_owned(),
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
					device_id: None,
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
				unique_props.insert(Prop {
					key: "level".into(),
					value: log.level.to_string(),
				});
			}
			self.db
				.upsert_segment_props(segment_id, unique_props.iter())
				.await
				.unwrap();
			let path = self.logs_path.join(format!("{}.log", segment_id));
			let mut file = File::create(&path).unwrap();
			file.write_all(&buff).unwrap();
			current.buffer.clear();
		}
	}

	pub async fn find_logs(
		&self,
		query: QueryAst,
		tx: &mpsc::Sender<LogEntry>,
	) -> anyhow::Result<()> {
		let mut end = query.end_date.unwrap_or(Utc::now());
		let device_ids = extract_device_ids(&query.root);
		let tz = query
			.tz_offset
			.unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
		let (mut start_bound, mut end_bound) = timestamp_bounds(&query.root);
		if let Some(e) = end_bound {
			if e < end {
				end = e;
			}
		}
		{
			let current = self.current.lock().await;
			let iter = current.iter();
			for entry in iter {
				if tx.is_closed() {
					return Ok(());
				}
				if entry.timestamp > end {
					continue;
				}
				if let Some(start) = start_bound {
					if entry.timestamp < start {
						continue;
					}
				}
				end = entry.timestamp;
				match check_expr(&query.root, entry, &tz) {
					Ok(true) => {}
					_ => continue,
				}
				if tx.send(entry.clone()).await.is_err() {
					return Ok(());
				}
			}
		}
		log::info!("looking from archive");
		let window = chrono::Duration::hours(24);
		let mut prev_end: Option<DateTime<Utc>> = Some(end);
		let mut processed_segments: HashSet<u32> = HashSet::new();

		'outer: loop {
			if tx.is_closed() {
				break;
			}
			let mut end = match self.db.prev_segment_end(prev_end).await? {
				Some(e) => e,
				None => {
					log::info!("no more segments to load");
					break;
				}
			};
			if let Some(start) = start_bound {
				if end < start {
					break;
				}
			}
			let mut start = end - window;
			if let Some(bound) = start_bound {
				if start < bound {
					start = bound;
				}
			}
			prev_end = Some(start);

			let timer = std::time::Instant::now();
			let segments = self
				.db
				.find_segments(&GetSegmentsQuery {
					start: Some(start),
					end: Some(end),
					device_ids: if device_ids.is_empty() {
						None
					} else {
						Some(device_ids.clone())
					},
					..Default::default()
				})
				.await
				.unwrap();
			if segments.is_empty() {
				log::info!("no segments found in the range {} - {}", start, end);
				break;
			}
			log::info!(
				"found {} segments in range {} - {} in {:?}",
				segments.len(),
				start,
				end,
				timer.elapsed()
			);
			for segment in &segments {
				if tx.is_closed() {
					break 'outer;
				}
				if !processed_segments.insert(segment.id) {
					continue;
				}
				let timer = std::time::Instant::now();
				let props = match self.db.fetch_segment_props(segment.id).await {
					Ok(props) => props,
					Err(err) => {
						log::error!("failed to fetch segment props: {}", err);
						continue;
					}
				};
				// First check whether the segment’s time window could satisfy the query.
				let time_match = match_date_range(
					&query.root,
					segment.first_timestamp,
					segment.last_timestamp,
					&tz,
				);
				if !time_match {
					end = segment.first_timestamp;
					continue;
				}

				// Only if the date range fits do we bother checking the segment’s properties.
				let prop_match = check_props(&query.root, &props).unwrap_or_default();
				if !prop_match {
					end = segment.first_timestamp;
					continue;
				}
				let path = self.logs_path.join(format!("{}.log", segment.id));
				log::info!(
					"loading {} segment {} - {}",
					segment.id,
					segment.first_timestamp,
					segment.last_timestamp
				);
				let file: File = match File::open(path) {
					Ok(file) => file,
					Err(err) => {
						log::error!("failed to open log file: {}", err);
						continue;
					}
				};
				let mut decoder = zstd::Decoder::new(file).unwrap();
				let segment = LogSegment::parse(&mut decoder);
				let iter = segment.iter();
				for entry in iter {
					if tx.is_closed() {
						break 'outer;
					}
					if entry.timestamp > end {
						continue;
					}
					match check_expr(&query.root, entry, &tz) {
						Ok(true) => {}
						_ => continue,
					}
					if tx.send(entry.clone()).await.is_err() {
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

	pub fn logs_path(&self) -> &Path {
		&self.logs_path
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

	// Helper to set up an isolated temp environment & Context for tests.
	async fn prepare_test_ctx() -> (Context, tempfile::TempDir) {
		use std::fs;
		use tempfile::tempdir;

		let dir = tempdir().unwrap();
		let logs_path = dir.path().join("logs");
		fs::create_dir_all(&logs_path).unwrap();
		let ctx = Context::new(logs_path).await;
		(ctx, dir)
	}

	#[tokio::test]
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
		let (tx, mut rx) = mpsc::channel(10);
		ctx.find_logs(query, &tx).await.unwrap();
		drop(tx);
		let mut found = Vec::new();
		while let Some(log) = rx.recv().await {
			found.push(log);
		}

		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "match me");
		drop(dir);
	}

	#[tokio::test]
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
				device_id: None,
				first_timestamp: entry.timestamp,
				last_timestamp: entry.timestamp,
				original_size,
				compressed_size,
				logs_count: 1,
			})
			.await
			.unwrap();

		let mut props_vec: Vec<Prop> = entry.props.clone();
		props_vec.push(Prop {
			key: "level".into(),
			value: entry.level.to_string(),
		});
		ctx.db
			.upsert_segment_props(segment_id, props_vec.iter())
			.await
			.unwrap();

		let path = ctx.logs_path.join(format!("{}.log", segment_id));
		fs::write(path, compressed).unwrap();

		let mut query = parse_log_query("msg = \"segment log\"").unwrap();
		query.end_date = Some(Utc::now());
		let (tx, mut rx) = mpsc::channel(10);
		ctx.find_logs(query, &tx).await.unwrap();
		drop(tx);
		let mut found = Vec::new();
		while let Some(log) = rx.recv().await {
			found.push(log);
		}

		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "segment log");
		drop(dir);
	}

	#[tokio::test]
	async fn find_logs_not_found() {
		let (ctx, dir) = prepare_test_ctx().await;

		// Search for a message that does not exist anywhere.
		let query = parse_log_query("msg = \"non‑existent log\"").unwrap();
		let (tx, mut rx) = mpsc::channel(10);
		ctx.find_logs(query, &tx).await.unwrap();
		drop(tx);
		let mut found = Vec::<LogEntry>::new();
		while let Some(log) = rx.recv().await {
			found.push(log);
		}

		// We expect to find **no** matching entries.
		assert!(found.is_empty(), "unexpectedly found logs: {:?}", found);
		drop(dir);
	}

	#[tokio::test]
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
				device_id: None,
				first_timestamp: entry_old.timestamp - Duration::hours(10),
				last_timestamp: entry_old.timestamp + Duration::hours(10),
				original_size: orig_size,
				compressed_size: comp_size,
				logs_count: 1,
			})
			.await
			.unwrap();
		let mut props_old_vec: Vec<Prop> = entry_old.props.clone();
		props_old_vec.push(Prop {
			key: "level".into(),
			value: entry_old.level.to_string(),
		});
		ctx.db
			.upsert_segment_props(old_seg_id, props_old_vec.iter())
			.await
			.unwrap();
		let path = ctx.logs_path.join(format!("{}.log", old_seg_id));
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
				device_id: None,
				first_timestamp: entry_new.timestamp,
				last_timestamp: entry_new.timestamp,
				original_size: orig_size,
				compressed_size: comp_size,
				logs_count: 1,
			})
			.await
			.unwrap();
		let mut props_new_vec: Vec<Prop> = entry_new.props.clone();
		props_new_vec.push(Prop {
			key: "level".into(),
			value: entry_new.level.to_string(),
		});
		ctx.db
			.upsert_segment_props(new_seg_id, props_new_vec.iter())
			.await
			.unwrap();
		let path = ctx.logs_path.join(format!("{}.log", new_seg_id));
		fs::write(path, compressed).unwrap();

		let mut query = parse_log_query("msg = \"duplicate\"").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(10);
		ctx.find_logs(query, &tx).await.unwrap();
		drop(tx);
		let mut found = Vec::new();
		while let Some(log) = rx.recv().await {
			found.push(log);
		}

		assert_eq!(found.len(), 1, "log returned more than once");
		assert_eq!(found[0].msg, "duplicate");
		drop(dir);
	}

	#[tokio::test]
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
					device_id: None,
					first_timestamp: skip_entry.timestamp,
					last_timestamp: skip_entry.timestamp,
					original_size,
					compressed_size,
					logs_count: 1,
				})
				.await
				.unwrap();

			let mut props: Vec<Prop> = skip_entry.props.clone();
			props.push(Prop {
				key: "level".into(),
				value: skip_entry.level.to_string(),
			});
			ctx.db.upsert_segment_props(id, props.iter()).await.unwrap();
			fs::write(ctx.logs_path.join(format!("{}.log", id)), compressed).unwrap();
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
					device_id: None,
					first_timestamp: want_entry.timestamp,
					last_timestamp: want_entry.timestamp,
					original_size,
					compressed_size,
					logs_count: 1,
				})
				.await
				.unwrap();
			let mut props_vec: Vec<Prop> = want_entry.props.clone();
			props_vec.push(Prop {
				key: "level".into(),
				value: want_entry.level.to_string(),
			});
			ctx.db
				.upsert_segment_props(id, props_vec.iter())
				.await
				.unwrap();
			fs::write(ctx.logs_path.join(format!("{}.log", id)), compressed).unwrap();
		}

		#[tokio::test]
		async fn find_logs_filter_device_id() {
			use chrono::{Duration, Utc};
			use puppylog::{parse_log_query, LogEntry, LogLevel, Prop};
			use std::fs;
			use std::io::Cursor;
			use zstd;

			let (ctx, dir) = prepare_test_ctx().await;
			let now = Utc::now();

			let entry1 = LogEntry {
				timestamp: now - Duration::hours(30),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "dev1".into(),
				}],
				msg: "d1".into(),
				..Default::default()
			};
			let mut seg1 = LogSegment::new();
			seg1.add_log_entry(entry1.clone());
			seg1.sort();
			let mut buff = Vec::new();
			seg1.serialize(&mut buff);
			let orig_size = buff.len();
			let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
			let comp_size = compressed.len();
			let seg_id1 = ctx
				.db
				.new_segment(NewSegmentArgs {
					device_id: Some("dev1".into()),
					first_timestamp: entry1.timestamp,
					last_timestamp: entry1.timestamp,
					original_size: orig_size,
					compressed_size: comp_size,
					logs_count: 1,
				})
				.await
				.unwrap();
			let mut props_vec: Vec<Prop> = entry1.props.clone();
			props_vec.push(Prop {
				key: "level".into(),
				value: entry1.level.to_string(),
			});
			ctx.db
				.upsert_segment_props(seg_id1, props_vec.iter())
				.await
				.unwrap();
			fs::write(ctx.logs_path.join(format!("{}.log", seg_id1)), compressed).unwrap();

			let entry2 = LogEntry {
				timestamp: now - Duration::hours(25),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "dev2".into(),
				}],
				msg: "d2".into(),
				..Default::default()
			};
			let mut seg2 = LogSegment::new();
			seg2.add_log_entry(entry2.clone());
			seg2.sort();
			let mut buff = Vec::new();
			seg2.serialize(&mut buff);
			let orig_size = buff.len();
			let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
			let comp_size = compressed.len();
			let seg_id2 = ctx
				.db
				.new_segment(NewSegmentArgs {
					device_id: Some("dev2".into()),
					first_timestamp: entry2.timestamp,
					last_timestamp: entry2.timestamp,
					original_size: orig_size,
					compressed_size: comp_size,
					logs_count: 1,
				})
				.await
				.unwrap();
			let mut props_vec: Vec<Prop> = entry2.props.clone();
			props_vec.push(Prop {
				key: "level".into(),
				value: entry2.level.to_string(),
			});
			ctx.db
				.upsert_segment_props(seg_id2, props_vec.iter())
				.await
				.unwrap();
			fs::write(ctx.logs_path.join(format!("{}.log", seg_id2)), compressed).unwrap();

			let mut query = parse_log_query("deviceId = dev1").unwrap();
			query.end_date = Some(now);
			let (tx, mut rx) = mpsc::channel(10);
			ctx.find_logs(query, &tx).await.unwrap();
			drop(tx);
			let mut found = Vec::new();
			while let Some(log) = rx.recv().await {
				found.push(log);
			}

			assert_eq!(found.len(), 1);
			assert_eq!(found[0].msg, "d1");
			drop(dir);
		}

		let q_skip = parse_log_query("msg = \"should_not_be_seen\"").unwrap();
		let (tx_skip, mut rx_skip) = mpsc::channel(10);
		ctx.find_logs(q_skip, &tx_skip).await.unwrap();
		drop(tx_skip);
		let mut seen = Vec::<LogEntry>::new();
		while let Some(e) = rx_skip.recv().await {
			seen.push(e);
		}
		assert!(
			seen.is_empty(),
			"search incorrectly visited segment {new_seg_id} that is newer than in-memory logs"
		);

		let q_target = parse_log_query("msg = \"target\"").unwrap();
		let (tx_target, mut rx_target) = mpsc::channel(10);
		ctx.find_logs(q_target, &tx_target).await.unwrap();
		drop(tx_target);
		let mut found = Vec::<LogEntry>::new();
		while let Some(e) = rx_target.recv().await {
			found.push(e);
		}
		assert_eq!(found.len(), 1);
		assert_eq!(found[0].msg, "target");
		drop(dir);
	}
}
