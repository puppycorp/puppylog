use crate::db::open_db;
use crate::db::NewSegmentArgs;
use crate::db::DB;
use crate::segment::compress_segment;
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
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

const CONCURRENCY_LIMIT: usize = 10;
/// Default number of buffered log entries before logs are flushed to disk.
pub const UPLOAD_FLUSH_THRESHOLD: usize = 3_000_000;

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
	upload_flush_threshold: AtomicUsize,
	logs_path: PathBuf,
	wal_max_bytes: u64,
	flush_interval: Duration,
	last_flush: StdMutex<Instant>,
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
		// Read optional env overrides for WAL/flush policy
		let wal_max_bytes = std::env::var("WAL_MAX_BYTES")
			.ok()
			.and_then(|v| v.parse::<u64>().ok())
			.unwrap_or(512 * 1024 * 1024); // 512 MB default
		let flush_interval = std::env::var("UPLOAD_FLUSH_INTERVAL_SECS")
			.ok()
			.and_then(|v| v.parse::<u64>().ok())
			.map(Duration::from_secs)
			.unwrap_or(Duration::from_secs(300)); // 5 minutes default

		Context {
			subscriber: Subscriber::new(subtx),
			publisher: pubtx,
			settings,
			event_tx,
			db,
			current: Mutex::new(LogSegment::with_logs(logs)),
			wal,
			upload_queue: AtomicUsize::new(0),
			upload_flush_threshold: AtomicUsize::new(UPLOAD_FLUSH_THRESHOLD),
			logs_path: logs_path.as_ref().to_owned(),
			wal_max_bytes,
			flush_interval,
			last_flush: StdMutex::new(Instant::now()),
		}
	}

	/// Override the default flush threshold for uploaded logs.
	pub fn set_upload_flush_threshold(&self, threshold: usize) {
		self.upload_flush_threshold
			.store(threshold, Ordering::Relaxed);
	}

	pub async fn save_logs(&self, logs: &[LogEntry]) {
		let mut current = self.current.lock().await;
		current.buffer.extend_from_slice(logs);
		for entry in logs {
			self.wal.write(entry.clone());
		}
		current.sort();
		let flush_threshold = self.upload_flush_threshold.load(Ordering::Relaxed);

		// Policy-based flush triggers: threshold, WAL size cap, or time interval
		let wal_size = std::fs::metadata(crate::wal::wal_path())
			.map(|m| m.len())
			.unwrap_or(0);
		let last_flush_elapsed = self
			.last_flush
			.lock()
			.map(|t| t.elapsed())
			.unwrap_or(Duration::from_secs(0));
		let policy_trigger =
			wal_size > self.wal_max_bytes || last_flush_elapsed >= self.flush_interval;

		if current.buffer.len() > flush_threshold || policy_trigger {
			self.flush_locked(&mut current).await;
		}
	}

	// Internal helper used by both save_logs (policy) and force_flush.
	async fn flush_locked(&self, current: &mut LogSegment) {
		if current.buffer.is_empty() {
			return;
		}
		self.wal.clear();

		// Group logs by device ID (or UNKNOWN_DEVICE_ID)
		let mut by_device: HashMap<String, Vec<LogEntry>> = HashMap::new();
		for log in current.buffer.drain(..) {
			let device_id = log
				.props
				.iter()
				.find(|p| p.key == "deviceId")
				.map(|p| p.value.clone())
				.unwrap_or_else(|| crate::dev_segment_merger::UNKNOWN_DEVICE_ID.to_string());
			by_device.entry(device_id).or_default().push(log);
		}

		for (device_id, mut logs) in by_device {
			logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
			let first_timestamp = logs.first().unwrap().timestamp;
			let last_timestamp = logs.last().unwrap().timestamp;
			let seg = LogSegment { buffer: logs };

			let mut buff = Vec::new();
			seg.serialize(&mut buff);
			let original_size = buff.len();
			let buff: Vec<u8> = match compress_segment(&buff) {
				Ok(compressed) => compressed,
				Err(e) => {
					log::error!("failed to compress segment: {}", e);
					continue;
				}
			};
			let compressed_size = buff.len();
			let segment_id = self
				.db
				.new_segment(NewSegmentArgs {
					device_id: Some(device_id.clone()),
					first_timestamp,
					last_timestamp,
					logs_count: seg.buffer.len() as u64,
					original_size,
					compressed_size,
				})
				.await
				.unwrap();
			let mut unique_props = HashSet::new();
			for log in &seg.buffer {
				unique_props.extend(log.props.iter().cloned());
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
		}

		if let Ok(mut t) = self.last_flush.lock() {
			*t = Instant::now();
		}
	}

	/// Force a flush of the current in-memory logs to segments and clear WAL,
	/// regardless of thresholds. No-ops if there are no buffered logs.
	pub async fn force_flush(&self) {
		let mut current = self.current.lock().await;
		self.flush_locked(&mut current).await;
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
		let (start_bound, end_bound) = timestamp_bounds(&query.root);
		log::info!(
			"start_bound = {:?}, end_bound = {:?}",
			start_bound,
			end_bound
		);
		if let Some(e) = end_bound {
			if e < end {
				end = e;
			}
		}
		{
			let mut end = end;
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
		log::info!("prev_end: {:?}", prev_end);

		'outer: loop {
			if tx.is_closed() {
				break;
			}
			let end_exists = self
				.db
				.segment_exists_at(
					prev_end.unwrap(),
					if device_ids.is_empty() {
						None
					} else {
						Some(&device_ids)
					},
				)
				.await?;
			let mut end = if end_exists {
				prev_end.unwrap()
			} else {
				match self
					.db
					.prev_segment_end(
						prev_end,
						if device_ids.is_empty() {
							None
						} else {
							Some(&device_ids)
						},
					)
					.await?
				{
					Some(e) => e,
					None => {
						log::info!("no more segments to load");
						break;
					}
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
	use crate::segment::compress_segment;
	use chrono::{Duration, Utc};
	use puppylog::{parse_log_query, LogEntry, LogLevel, Prop};
	use std::fs;

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
		let compressed = compress_segment(&buff).unwrap();
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
		let compressed = compress_segment(&buff).unwrap();
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
		let compressed = compress_segment(&buff).unwrap();
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
	async fn find_logs_filter_device_id() {
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
		let compressed = compress_segment(&buff).unwrap();
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
		let compressed = compress_segment(&buff).unwrap();
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

	#[tokio::test]
	async fn find_logs_device_gap() {
		let (ctx, dir) = prepare_test_ctx().await;
		let now = Utc::now();

		let entry_old = LogEntry {
			timestamp: now - Duration::hours(30),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "deviceId".into(),
				value: "dev1".into(),
			}],
			msg: "old".into(),
			..Default::default()
		};
		let mut seg_old = LogSegment::new();
		seg_old.add_log_entry(entry_old.clone());
		seg_old.sort();
		let mut buff = Vec::new();
		seg_old.serialize(&mut buff);
		let orig_size = buff.len();
		let compressed = compress_segment(&buff).unwrap();
		let comp_size = compressed.len();
		let seg_old_id = ctx
			.db
			.new_segment(NewSegmentArgs {
				device_id: Some("dev1".into()),
				first_timestamp: entry_old.timestamp,
				last_timestamp: entry_old.timestamp,
				original_size: orig_size,
				compressed_size: comp_size,
				logs_count: 1,
			})
			.await
			.unwrap();
		let mut props_vec: Vec<Prop> = entry_old.props.clone();
		props_vec.push(Prop {
			key: "level".into(),
			value: entry_old.level.to_string(),
		});
		ctx.db
			.upsert_segment_props(seg_old_id, props_vec.iter())
			.await
			.unwrap();
		fs::write(
			ctx.logs_path.join(format!("{}.log", seg_old_id)),
			compressed,
		)
		.unwrap();

		let entry_other = LogEntry {
			timestamp: now - Duration::hours(5),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "deviceId".into(),
				value: "dev2".into(),
			}],
			msg: "other".into(),
			..Default::default()
		};
		let mut seg_other = LogSegment::new();
		seg_other.add_log_entry(entry_other.clone());
		seg_other.sort();
		let mut buff = Vec::new();
		seg_other.serialize(&mut buff);
		let orig_size = buff.len();
		let compressed = compress_segment(&buff).unwrap();
		let comp_size = compressed.len();
		let seg_other_id = ctx
			.db
			.new_segment(NewSegmentArgs {
				device_id: Some("dev2".into()),
				first_timestamp: entry_other.timestamp,
				last_timestamp: entry_other.timestamp,
				original_size: orig_size,
				compressed_size: comp_size,
				logs_count: 1,
			})
			.await
			.unwrap();
		let mut props_vec2: Vec<Prop> = entry_other.props.clone();
		props_vec2.push(Prop {
			key: "level".into(),
			value: entry_other.level.to_string(),
		});
		ctx.db
			.upsert_segment_props(seg_other_id, props_vec2.iter())
			.await
			.unwrap();
		fs::write(
			ctx.logs_path.join(format!("{}.log", seg_other_id)),
			compressed,
		)
		.unwrap();

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
		assert_eq!(found[0].msg, "old");
		drop(dir);
	}

	#[tokio::test]
	async fn save_logs_flushes_by_device() {
		let (ctx, _dir) = prepare_test_ctx().await;
		let now = Utc::now();
		ctx.set_upload_flush_threshold(10);

		const PER_DEVICE: usize = 6; // ensures total > threshold
		let mut logs = Vec::with_capacity(PER_DEVICE * 2);
		for i in 0..PER_DEVICE {
			logs.push(LogEntry {
				timestamp: now + Duration::seconds(i as i64),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "devA".into(),
				}],
				msg: String::new(),
				..Default::default()
			});
		}
		for i in 0..PER_DEVICE {
			logs.push(LogEntry {
				timestamp: now + Duration::seconds((PER_DEVICE + i) as i64),
				level: LogLevel::Warn,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "devB".into(),
				}],
				msg: String::new(),
				..Default::default()
			});
		}

		ctx.save_logs(&logs).await;

		let segs = ctx
			.db
			.find_segments(&GetSegmentsQuery::default())
			.await
			.unwrap();
		assert_eq!(segs.len(), 2); // one per device
		for seg in &segs {
			match seg.device_id.as_deref() {
				Some("devA") => assert_eq!(seg.logs_count, PER_DEVICE as u64),
				Some("devB") => assert_eq!(seg.logs_count, PER_DEVICE as u64),
				other => panic!("unexpected device id {other:?}"),
			}
		}
	}

	#[tokio::test]
	async fn save_logs_unknown_device() {
		let (ctx, _dir) = prepare_test_ctx().await;
		let now = Utc::now();
		ctx.set_upload_flush_threshold(10);

		const COUNT: usize = 11; // trigger flush
		let mut logs = Vec::with_capacity(COUNT);
		for i in 0..COUNT {
			logs.push(LogEntry {
				timestamp: now + Duration::seconds(i as i64),
				level: LogLevel::Info,
				props: vec![],
				msg: String::new(),
				..Default::default()
			});
		}

		ctx.save_logs(&logs).await;

		let segs = ctx
			.db
			.find_segments(&GetSegmentsQuery::default())
			.await
			.unwrap();
		assert_eq!(segs.len(), 1);
		assert_eq!(
			segs[0].device_id.as_deref(),
			Some(crate::dev_segment_merger::UNKNOWN_DEVICE_ID)
		);
		assert_eq!(segs[0].logs_count, COUNT as u64);
	}

	#[tokio::test]
	async fn find_logs_pagination_resume_segment() {
		use std::sync::Arc;

		let (ctx, dir) = prepare_test_ctx().await;
		let ctx = Arc::new(ctx);
		let now = Utc::now();

		let ts0 = now - Duration::hours(5);
		let ts1 = now - Duration::hours(6);
		let ts2 = now - Duration::hours(7);

		let logs = vec![
			LogEntry {
				timestamp: ts0,
				level: LogLevel::Info,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "dev1".into(),
				}],
				msg: "l0".into(),
				..Default::default()
			},
			LogEntry {
				timestamp: ts1,
				level: LogLevel::Info,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "dev1".into(),
				}],
				msg: "l1".into(),
				..Default::default()
			},
			LogEntry {
				timestamp: ts2,
				level: LogLevel::Info,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "dev1".into(),
				}],
				msg: "l2".into(),
				..Default::default()
			},
		];

		let mut seg = LogSegment::new();
		for log in &logs {
			seg.add_log_entry(log.clone());
		}
		seg.sort();
		let mut buff = Vec::new();
		seg.serialize(&mut buff);
		let orig_size = buff.len();
		let compressed = compress_segment(&buff).unwrap();
		let comp_size = compressed.len();
		let seg_id = ctx
			.db
			.new_segment(NewSegmentArgs {
				device_id: Some("dev1".into()),
				first_timestamp: ts2,
				last_timestamp: ts0,
				original_size: orig_size,
				compressed_size: comp_size,
				logs_count: logs.len() as u64,
			})
			.await
			.unwrap();
		ctx.db
			.upsert_segment_props(
				seg_id,
				[
					Prop {
						key: "deviceId".into(),
						value: "dev1".into(),
					},
					Prop {
						key: "level".into(),
						value: LogLevel::Info.to_string(),
					},
				]
				.iter(),
			)
			.await
			.unwrap();
		fs::write(ctx.logs_path.join(format!("{}.log", seg_id)), compressed).unwrap();

		let mut query = parse_log_query("deviceId = dev1").unwrap();
		query.end_date = Some(ts0);
		let (tx, mut rx) = mpsc::channel(10);
		let ctx_clone = Arc::clone(&ctx);
		let handle = tokio::spawn(async move {
			ctx_clone.find_logs(query, &tx).await.unwrap();
		});
		let first = rx.recv().await.unwrap();
		drop(rx);
		handle.await.unwrap();

		assert_eq!(first.timestamp.timestamp_millis(), ts0.timestamp_millis());

		let mut query2 = parse_log_query("deviceId = dev1").unwrap();
		query2.end_date = Some(first.timestamp - Duration::microseconds(1));
		let (tx2, mut rx2) = mpsc::channel(10);
		ctx.find_logs(query2, &tx2).await.unwrap();
		drop(tx2);
		let mut remaining = Vec::new();
		while let Some(log) = rx2.recv().await {
			remaining.push(log.timestamp);
		}

		remaining.sort();
		assert_eq!(remaining.len(), 2);
		assert_eq!(remaining[0].timestamp_millis(), ts2.timestamp_millis());
		assert_eq!(remaining[1].timestamp_millis(), ts1.timestamp_millis());
		drop(dir);
	}
}
