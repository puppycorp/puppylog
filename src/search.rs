use chrono::{DateTime, Utc};
use puppylog::{check_expr, check_props, extract_device_ids, timestamp_bounds, LogEntry, QueryAst};
use std::collections::HashSet;
use std::fs::File;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::timeout;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::db::DB;
use crate::segment::LogSegment;
use crate::types::GetSegmentsQuery;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentProgress {
	pub segment_id: u32,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub device_id: Option<String>,
	pub first_timestamp: DateTime<Utc>,
	pub last_timestamp: DateTime<Utc>,
	pub logs_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgress {
	pub processed_logs: u64,
	pub logs_per_second: f64,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LogStreamItem {
	Entry(LogEntry),
	SegmentProgress(SegmentProgress),
	SearchProgress(SearchProgress),
}

fn calculate_logs_per_second(processed_logs: u64, search_start: Instant) -> f64 {
	if processed_logs == 0 {
		return 0.0;
	}
	let seconds = search_start.elapsed().as_secs_f64();
	if seconds > 0.0 {
		processed_logs as f64 / seconds
	} else {
		0.0
	}
}

async fn send_search_progress(
	tx: &mpsc::Sender<LogStreamItem>,
	processed_logs: u64,
	logs_per_second: f64,
	status: Option<&str>,
) -> bool {
	tx.send(LogStreamItem::SearchProgress(SearchProgress {
		processed_logs,
		logs_per_second,
		status: status.map(|s| s.to_string()),
	}))
	.await
	.is_err()
}

fn should_emit_progress(processed_logs: u64, last_emit: &Instant) -> bool {
	processed_logs == 0
		|| processed_logs == 1
		|| processed_logs % 1_000 == 0
		|| last_emit.elapsed() >= Duration::from_millis(500)
}

pub struct LogSearcher<'a> {
	pub db: &'a DB,
	pub current: &'a Mutex<LogSegment>,
	pub logs_path: &'a Path,
	/// How wide the archive search window is when walking backwards.
	pub window: chrono::Duration,
}

impl<'a> LogSearcher<'a> {
	pub fn new(db: &'a DB, current: &'a Mutex<LogSegment>, logs_path: &'a Path) -> Self {
		Self {
			db,
			current,
			logs_path,
			window: chrono::Duration::hours(24),
		}
	}

	pub async fn search(
		&self,
		query: QueryAst,
		tx: &mpsc::Sender<LogStreamItem>,
	) -> anyhow::Result<()> {
		let search_start = Instant::now();
		let mut processed_logs: u64 = 0;
		let mut last_progress_emit = search_start;

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

		// 1) Search in-memory buffer (with timeout on the lock)
		match timeout(Duration::from_millis(100), self.current.lock()).await {
			Ok(current) => {
				let mut end = end;
				let iter = current.iter();
				for entry in iter {
					if tx.is_closed() {
						return Ok(());
					}
					processed_logs += 1;
					if should_emit_progress(processed_logs, &last_progress_emit) {
						let speed = calculate_logs_per_second(processed_logs, search_start);
						if send_search_progress(tx, processed_logs, speed, None).await {
							return Ok(());
						}
						last_progress_emit = Instant::now();
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
					if tx.send(LogStreamItem::Entry(entry.clone())).await.is_err() {
						return Ok(());
					}
				}
			}
			Err(_) => {
				if last_progress_emit.elapsed() >= Duration::from_millis(500) {
					let speed = calculate_logs_per_second(processed_logs, search_start);
					if send_search_progress(
						tx,
						processed_logs,
						speed,
						Some("waiting for in-memory log buffer"),
					)
					.await
					{
						return Ok(());
					}
					last_progress_emit = Instant::now();
				}
			}
		}

		// 2) Search archived segments on disk
		log::info!("looking from archive");
		let window = self.window;
		let mut prev_end: Option<DateTime<Utc>> = Some(end);
		let mut processed_segments: HashSet<u32> = HashSet::new();
		log::info!("prev_end: {:?}", prev_end);

		'outer: loop {
			if tx.is_closed() {
				break;
			}
			let current_prev = match prev_end {
				Some(ts) => ts,
				None => {
					log::info!("no previous end; stopping");
					break;
				}
			};
			let end_exists = self
				.db
				.segment_exists_at(
					current_prev,
					if device_ids.is_empty() {
						None
					} else {
						Some(&device_ids)
					},
				)
				.await?;
			let mut end = if end_exists {
				current_prev
			} else {
				match self
					.db
					.prev_segment_end(
						Some(&current_prev),
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
			if last_progress_emit.elapsed() >= Duration::from_millis(500) {
				let speed = calculate_logs_per_second(processed_logs, search_start);
				if send_search_progress(
					tx,
					processed_logs,
					speed,
					Some("loading matching segments"),
				)
				.await
				{
					break;
				}
				last_progress_emit = Instant::now();
			}
			let segments = match self
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
			{
				Ok(segments) => segments,
				Err(err) => {
					log::error!("failed to load segments: {}", err);
					return Err(err);
				}
			};
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
				if last_progress_emit.elapsed() >= Duration::from_millis(500) {
					let speed = calculate_logs_per_second(processed_logs, search_start);
					if send_search_progress(
						tx,
						processed_logs,
						speed,
						Some("loading segment metadata"),
					)
					.await
					{
						break 'outer;
					}
					last_progress_emit = Instant::now();
				}
				let props = match self.db.fetch_segment_props(segment.id).await {
					Ok(props) => props,
					Err(err) => {
						log::error!("failed to fetch segment props: {}", err);
						continue;
					}
				};
				// Check whether the segment's time window could satisfy the query.
				let time_match = puppylog::match_date_range(
					&query.root,
					segment.first_timestamp,
					segment.last_timestamp,
					&tz,
				);
				if !time_match {
					// Only time mismatch changes the `end` scan position.
					end = segment.first_timestamp;
					continue;
				}

				// Only if the date range fits do we bother checking the segment's properties.
				let prop_match = check_props(&query.root, &props).unwrap_or_default();
				if !prop_match {
					// IMPORTANT: do NOT move `end` here; otherwise other devices'
					// segments will cut off later logs for the target device.
					continue;
				}
				if tx
					.send(LogStreamItem::SegmentProgress(SegmentProgress {
						segment_id: segment.id,
						device_id: segment.device_id.clone(),
						first_timestamp: segment.first_timestamp,
						last_timestamp: segment.last_timestamp,
						logs_count: segment.logs_count,
					}))
					.await
					.is_err()
				{
					break 'outer;
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
					processed_logs += 1;
					if should_emit_progress(processed_logs, &last_progress_emit) {
						let speed = calculate_logs_per_second(processed_logs, search_start);
						if send_search_progress(tx, processed_logs, speed, None).await {
							break 'outer;
						}
						last_progress_emit = Instant::now();
					}
					if entry.timestamp > end {
						continue;
					}
					match check_expr(&query.root, entry, &tz) {
						Ok(true) => {}
						_ => continue,
					}
					if tx.send(LogStreamItem::Entry(entry.clone())).await.is_err() {
						log::info!("stopped searching logs at {:?}", entry);
						break 'outer;
					}
				}
			}
		}

		if processed_logs > 0 {
			let logs_per_second = calculate_logs_per_second(processed_logs, search_start);
			let _ = tx
				.send(LogStreamItem::SearchProgress(SearchProgress {
					processed_logs,
					logs_per_second,
					status: None,
				}))
				.await;
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::db::{open_db, NewSegmentArgs};
	use crate::segment::compress_segment;
	use chrono::{Duration, Utc};
	use puppylog::{parse_log_query, LogEntry, LogLevel, Prop};
	use std::fs;
	use std::path::PathBuf;
	use tempfile::TempDir;

	struct TestSearcherEnv {
		db: DB,
		current: Mutex<LogSegment>,
		logs_path: PathBuf,
		_tempdir: TempDir,
	}

	impl TestSearcherEnv {
		fn new() -> Self {
			let tempdir = TempDir::new().unwrap();
			let logs_path = tempdir.path().join("logs");
			fs::create_dir_all(&logs_path).unwrap();
			let db = DB::new(open_db());
			Self {
				db,
				current: Mutex::new(LogSegment::new()),
				logs_path,
				_tempdir: tempdir,
			}
		}

		fn searcher(&self) -> LogSearcher<'_> {
			LogSearcher::new(&self.db, &self.current, &self.logs_path)
		}

		async fn persist_segment(&self, entry: &LogEntry, device_id: Option<&str>) -> u32 {
			let mut segment = LogSegment::new();
			segment.add_log_entry(entry.clone());
			segment.sort();
			let mut buff = Vec::new();
			segment.serialize(&mut buff);
			let original_size = buff.len();
			let compressed = compress_segment(&buff).unwrap();
			let compressed_size = compressed.len();
			let segment_id = self
				.db
				.new_segment(NewSegmentArgs {
					device_id: device_id.map(|id| id.to_string()),
					first_timestamp: entry.timestamp,
					last_timestamp: entry.timestamp,
					original_size,
					compressed_size,
					logs_count: 1,
				})
				.await
				.unwrap();
			let mut props_vec = entry.props.clone();
			props_vec.push(Prop {
				key: "level".into(),
				value: entry.level.to_string(),
			});
			self.db
				.upsert_segment_props(segment_id, props_vec.iter())
				.await
				.unwrap();
			fs::write(
				self.logs_path.join(format!("{}.log", segment_id)),
				compressed,
			)
			.unwrap();
			segment_id
		}
	}

	#[tokio::test]
	async fn search_returns_logs_from_memory() {
		let env = TestSearcherEnv::new();
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "service".into(),
				value: "memory".into(),
			}],
			msg: "from-memory".into(),
			..Default::default()
		};
		{
			let mut current = env.current.lock().await;
			current.add_log_entry(entry.clone());
			current.sort();
		}

		let mut query = parse_log_query("msg = \"from-memory\"").unwrap();
		query.end_date = Some(entry.timestamp + Duration::seconds(1));
		let (tx, mut rx) = mpsc::channel(4);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "from-memory");
	}

	#[tokio::test]
	async fn search_reads_from_archived_segment() {
		let env = TestSearcherEnv::new();
		let entry = LogEntry {
			timestamp: Utc::now() - Duration::hours(48),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "service".into(),
				value: "segment".into(),
			}],
			msg: "segment-log".into(),
			..Default::default()
		};
		env.persist_segment(&entry, None).await;

		let mut query = parse_log_query("msg = \"segment-log\"").unwrap();
		query.end_date = Some(Utc::now());
		let (tx, mut rx) = mpsc::channel(4);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "segment-log");
	}

	#[tokio::test]
	async fn search_filters_segments_by_device() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();
		let entry1 = LogEntry {
			timestamp: now - Duration::hours(30),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "deviceId".into(),
				value: "dev1".into(),
			}],
			msg: "only-me".into(),
			..Default::default()
		};
		let seg1 = env.persist_segment(&entry1, Some("dev1")).await;

		let entry2 = LogEntry {
			timestamp: now - Duration::hours(28),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "deviceId".into(),
				value: "dev2".into(),
			}],
			msg: "ignore-me".into(),
			..Default::default()
		};
		env.persist_segment(&entry2, Some("dev2")).await;

		let mut query = parse_log_query("deviceId = dev1").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(8);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		let mut segments = Vec::new();
		while let Some(item) = rx.recv().await {
			match item {
				LogStreamItem::Entry(log) => entries.push(log),
				LogStreamItem::SegmentProgress(progress) => segments.push(progress.segment_id),
				_ => {}
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "only-me");
		assert_eq!(segments, vec![seg1]);
	}

	#[tokio::test]
	async fn search_filters_by_level() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		// Add logs with different levels to memory
		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(3),
				level: LogLevel::Error,
				props: vec![],
				msg: "error-log".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(2),
				level: LogLevel::Info,
				props: vec![],
				msg: "info-log".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Debug,
				props: vec![],
				msg: "debug-log".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("level = error").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "error-log");
		assert_eq!(entries[0].level, LogLevel::Error);
	}

	#[tokio::test]
	async fn search_filters_by_property() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(2),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "service".into(),
					value: "auth".into(),
				}],
				msg: "auth-log".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "service".into(),
					value: "api".into(),
				}],
				msg: "api-log".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("service = auth").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "auth-log");
	}

	#[tokio::test]
	async fn search_combines_memory_and_archive() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		// Add a log entry to archived segment
		let archived_entry = LogEntry {
			timestamp: now - Duration::hours(30),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "source".into(),
				value: "test".into(),
			}],
			msg: "archived-log".into(),
			..Default::default()
		};
		env.persist_segment(&archived_entry, None).await;

		// Add a log entry to memory
		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "source".into(),
					value: "test".into(),
				}],
				msg: "memory-log".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("source = test").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 2);
		// Results should be in reverse chronological order (newest first)
		assert_eq!(entries[0].msg, "memory-log");
		assert_eq!(entries[1].msg, "archived-log");
	}

	#[tokio::test]
	async fn search_returns_empty_for_no_matches() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Info,
				props: vec![],
				msg: "existing-log".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("msg = \"nonexistent\"").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert!(entries.is_empty());
	}

	#[tokio::test]
	async fn search_emits_progress_events() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Info,
				props: vec![],
				msg: "test-log".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("level = info").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut progress_count = 0;
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::SearchProgress(_) = item {
				progress_count += 1;
			}
		}

		// Should have at least one progress event
		assert!(progress_count >= 1);
	}

	#[tokio::test]
	async fn search_emits_segment_progress() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		let entry = LogEntry {
			timestamp: now - Duration::hours(30),
			level: LogLevel::Info,
			props: vec![],
			msg: "segment-log".into(),
			..Default::default()
		};
		let seg_id = env.persist_segment(&entry, None).await;

		let mut query = parse_log_query("msg = \"segment-log\"").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut segment_progress = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::SegmentProgress(progress) = item {
				segment_progress.push(progress);
			}
		}

		assert_eq!(segment_progress.len(), 1);
		assert_eq!(segment_progress[0].segment_id, seg_id);
		assert_eq!(segment_progress[0].logs_count, 1);
	}

	#[tokio::test]
	async fn search_handles_multiple_segments() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		// Create multiple segments at different times
		for i in 1..=3 {
			let entry = LogEntry {
				timestamp: now - Duration::hours(i * 10),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "batch".into(),
					value: "test".into(),
				}],
				msg: format!("log-{}", i),
				..Default::default()
			};
			env.persist_segment(&entry, None).await;
		}

		let mut query = parse_log_query("batch = test").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(32);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		let mut segments = Vec::new();
		while let Some(item) = rx.recv().await {
			match item {
				LogStreamItem::Entry(log) => entries.push(log),
				LogStreamItem::SegmentProgress(progress) => segments.push(progress.segment_id),
				_ => {}
			}
		}

		assert_eq!(entries.len(), 3);
		assert_eq!(segments.len(), 3);
	}

	#[tokio::test]
	async fn search_stops_when_channel_closed() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		// Add multiple entries to memory
		{
			let mut current = env.current.lock().await;
			for i in 0..100 {
				current.add_log_entry(LogEntry {
					timestamp: now - Duration::seconds(i),
					level: LogLevel::Info,
					props: vec![],
					msg: format!("log-{}", i),
					..Default::default()
				});
			}
			current.sort();
		}

		let mut query = parse_log_query("level = info").unwrap();
		query.end_date = Some(now);
		let (tx, rx) = mpsc::channel(1);

		// Drop the receiver immediately to close the channel
		drop(rx);

		// Search should complete without error even though channel is closed
		let result = env.searcher().search(query, &tx).await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn search_with_custom_window() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		// Create segment outside default 24h window but within custom window
		let entry = LogEntry {
			timestamp: now - Duration::hours(48),
			level: LogLevel::Info,
			props: vec![],
			msg: "old-log".into(),
			..Default::default()
		};
		env.persist_segment(&entry, None).await;

		let mut searcher = env.searcher();
		searcher.window = chrono::Duration::hours(72);

		let mut query = parse_log_query("msg = \"old-log\"").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		searcher.search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "old-log");
	}

	#[tokio::test]
	async fn search_with_msg_like_pattern() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(3),
				level: LogLevel::Error,
				props: vec![],
				msg: "connection error: timeout".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(2),
				level: LogLevel::Info,
				props: vec![],
				msg: "connection established".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Debug,
				props: vec![],
				msg: "debug info".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("msg like \"connection\"").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 2);
		assert!(entries.iter().all(|e| e.msg.contains("connection")));
	}

	#[tokio::test]
	async fn search_with_and_condition() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(3),
				level: LogLevel::Error,
				props: vec![Prop {
					key: "service".into(),
					value: "auth".into(),
				}],
				msg: "auth error".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(2),
				level: LogLevel::Info,
				props: vec![Prop {
					key: "service".into(),
					value: "auth".into(),
				}],
				msg: "auth success".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Error,
				props: vec![Prop {
					key: "service".into(),
					value: "api".into(),
				}],
				msg: "api error".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("level = error and service = auth").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "auth error");
	}

	#[tokio::test]
	async fn search_with_or_condition() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(3),
				level: LogLevel::Error,
				props: vec![],
				msg: "error log".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(2),
				level: LogLevel::Warn,
				props: vec![],
				msg: "warn log".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::seconds(1),
				level: LogLevel::Info,
				props: vec![],
				msg: "info log".into(),
				..Default::default()
			});
			current.sort();
		}

		let mut query = parse_log_query("level = error or level = warn").unwrap();
		query.end_date = Some(now);
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 2);
		assert!(entries
			.iter()
			.any(|e| e.level == LogLevel::Error));
		assert!(entries.iter().any(|e| e.level == LogLevel::Warn));
	}

	#[tokio::test]
	async fn search_respects_end_date_boundary() {
		let env = TestSearcherEnv::new();
		let now = Utc::now();

		{
			let mut current = env.current.lock().await;
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::hours(2),
				level: LogLevel::Info,
				props: vec![],
				msg: "old-log".into(),
				..Default::default()
			});
			current.add_log_entry(LogEntry {
				timestamp: now - Duration::minutes(30),
				level: LogLevel::Info,
				props: vec![],
				msg: "recent-log".into(),
				..Default::default()
			});
			current.sort();
		}

		// Set end_date to 1 hour ago, should exclude "recent-log"
		let mut query = parse_log_query("level = info").unwrap();
		query.end_date = Some(now - Duration::hours(1));
		let (tx, mut rx) = mpsc::channel(16);
		env.searcher().search(query, &tx).await.unwrap();
		drop(tx);

		let mut entries = Vec::new();
		while let Some(item) = rx.recv().await {
			if let LogStreamItem::Entry(log) = item {
				entries.push(log);
			}
		}

		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].msg, "old-log");
	}
}
