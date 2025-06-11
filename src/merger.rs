use std::collections::HashSet;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use rand::Rng;
use tokio::time::sleep;

use crate::context::Context;
use crate::db::NewSegmentArgs;
use crate::segment::{LogSegment, SegmentMeta};
use crate::types::{GetSegmentsQuery, SortDir};
use puppylog::{LogEntry, Prop};

const TARGET_LOGS: usize = 100_000;

/// Return windows of consecutive indices that should be merged.
/// Each window contains at least two segments whose timestamp
/// ranges overlap and whose combined log count does not exceed
/// [`TARGET_LOGS`].
fn windows_for_merge(metas: &[SegmentMeta]) -> Vec<(usize, usize)> {
	let mut out = Vec::new();
	if metas.is_empty() {
		return out;
	}

	let mut start = 0;
	let mut total_logs = metas[0].logs_count as usize;
	let mut max_end = metas[0].last_timestamp;
	let mut has_overlap = false;

	for (end, m) in metas.iter().enumerate().skip(1) {
		let overlaps = m.first_timestamp <= max_end;
		let fits = total_logs + m.logs_count as usize <= TARGET_LOGS;

		if overlaps && fits {
			total_logs += m.logs_count as usize;
			max_end = max_end.max(m.last_timestamp);
			has_overlap = true;
		} else {
			if has_overlap && end - start > 1 {
				out.push((start, end));
			}
			start = end;
			total_logs = m.logs_count as usize;
			max_end = m.last_timestamp;
			has_overlap = false;
		}
	}

	if has_overlap && metas.len() - start > 1 {
		out.push((start, metas.len()));
	}

	out
}

pub async fn merge_segments(ctx: Arc<Context>) {
	loop {
		if let Err(e) = merge_once(&ctx).await {
			log::error!("merge_segments: {}", e);
		}
		sleep(Duration::from_secs(60)).await;
	}
}

async fn merge_once(ctx: &Arc<Context>) -> anyhow::Result<()> {
	let segments = ctx
		.db
		.find_segments(&GetSegmentsQuery {
			start: None,
			end: None,
			count: None,
			sort: Some(SortDir::Asc),
		})
		.await?;
	if segments.len() < 2 {
		return Ok(());
	}

	let windows = windows_for_merge(&segments);
	if windows.is_empty() {
		return Ok(());
	}

	let log_dir = ctx.logs_dir();
	for (lo, hi) in windows {
		let mut logs = Vec::new();
		let mut ids = Vec::new();
		for meta in &segments[lo..hi] {
			let path = log_dir.join(format!("{}.log", meta.id));
			let compressed = tokio::fs::read(&path).await?;
			let decoded = zstd::decode_all(Cursor::new(compressed))?;
			let mut cursor = Cursor::new(decoded);
			let seg = LogSegment::parse(&mut cursor);
			logs.extend(seg.buffer.into_iter());
			ids.push(meta.id);
		}
		write_segment(ctx, &logs, &ids, log_dir).await?;
	}
	Ok(())
}

async fn write_segment(
	ctx: &Arc<Context>,
	logs: &[LogEntry],
	old_ids: &[u32],
	log_dir: &std::path::Path,
) -> anyhow::Result<()> {
	if logs.is_empty() {
		return Ok(());
	}
	let mut segment = LogSegment::with_logs(logs.to_vec());
	segment.sort();
	let first_timestamp = segment.buffer.first().unwrap().timestamp;
	let last_timestamp = segment.buffer.last().unwrap().timestamp;
	let mut buff = Cursor::new(Vec::new());
	segment.serialize(&mut buff);
	let original_size = buff.position() as usize;
	buff.set_position(0);
	let compressed = zstd::encode_all(buff, 0)?;
	let compressed_size = compressed.len();

	let tmp_name = format!("merge-{}.tmp", rand::thread_rng().gen::<u64>());
	let tmp_path = log_dir.join(&tmp_name);
	tokio::fs::write(&tmp_path, &compressed).await?;

	let new_id = ctx
		.db
		.new_segment(NewSegmentArgs {
			first_timestamp,
			last_timestamp,
			original_size,
			compressed_size,
			logs_count: segment.buffer.len() as u64,
		})
		.await?;
	let mut props = HashSet::new();
	for entry in &segment.buffer {
		for prop in &entry.props {
			props.insert(prop.clone());
		}
		props.insert(Prop {
			key: "level".into(),
			value: entry.level.to_string(),
		});
	}
	ctx.db.upsert_segment_props(new_id, props.iter()).await?;
	tokio::fs::rename(&tmp_path, log_dir.join(format!("{}.log", new_id))).await?;
	for id in old_ids {
		ctx.db.delete_segment(*id).await?;
		let _ = tokio::fs::remove_file(log_dir.join(format!("{}.log", id))).await;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::context::Context;
	use chrono::{Duration, Utc};
	use puppylog::{LogEntry, LogLevel, Prop};
	use serial_test::serial;
	use std::fs;
	use tempfile::tempdir;

	async fn prepare_test_ctx() -> (Arc<Context>, tempfile::TempDir) {
		let dir = tempdir().unwrap();
		let logs_dir = dir.path().join("logs");
		fs::create_dir_all(&logs_dir).unwrap();
		let ctx = Arc::new(Context::new(&logs_dir).await);
		(ctx, dir)
	}

	async fn create_segment(ctx: &Arc<Context>, entry: LogEntry) -> u32 {
		use std::io::Cursor;

		let mut seg = LogSegment::new();
		seg.add_log_entry(entry.clone());
		seg.sort();
		let mut buff = Vec::new();
		seg.serialize(&mut buff);
		let original_size = buff.len();
		let compressed = zstd::encode_all(Cursor::new(buff), 0).unwrap();
		let compressed_size = compressed.len();
		let id = ctx
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
			.upsert_segment_props(
				id,
				[Prop {
					key: "level".into(),
					value: entry.level.to_string(),
				}]
				.iter(),
			)
			.await
			.unwrap();
		tokio::fs::write(ctx.logs_dir().join(format!("{}.log", id)), compressed)
			.await
			.unwrap();
		id
	}

	#[tokio::test]
	#[serial]
	async fn merge_segments_combines_overlapping_segments() {
		let (ctx, _dir) = prepare_test_ctx().await;
		let now = Utc::now();

		let id1 = create_segment(
			&ctx,
			LogEntry {
				timestamp: now,
				level: LogLevel::Info,
				msg: "first".into(),
				..Default::default()
			},
		)
		.await;
		let id2 = create_segment(
			&ctx,
			LogEntry {
				timestamp: now,
				level: LogLevel::Info,
				msg: "second".into(),
				..Default::default()
			},
		)
		.await;

		// Ensure we start with two segments
		assert_eq!(
			ctx.db
				.find_segments(&GetSegmentsQuery {
					start: None,
					end: None,
					count: None,
					sort: Some(SortDir::Asc),
				})
				.await
				.unwrap()
				.len(),
			2
		);

		merge_once(&ctx).await.unwrap();

		let metas = ctx
			.db
			.find_segments(&GetSegmentsQuery {
				start: None,
				end: None,
				count: None,
				sort: Some(SortDir::Asc),
			})
			.await
			.unwrap();
		assert_eq!(metas.len(), 1);
		let merged_id = metas[0].id;
		assert_eq!(metas[0].logs_count, 2);

		// Old segment files should be gone
		assert!(!ctx.logs_dir().join(format!("{}.log", id1)).exists());
		assert!(!ctx.logs_dir().join(format!("{}.log", id2)).exists());
		assert!(ctx.logs_dir().join(format!("{}.log", merged_id)).exists());

		let compressed = fs::read(ctx.logs_dir().join(format!("{}.log", merged_id))).unwrap();
		let decoded = zstd::decode_all(Cursor::new(compressed)).unwrap();
		let mut cursor = Cursor::new(decoded);
		let seg = LogSegment::parse(&mut cursor);
		assert_eq!(seg.buffer.len(), 2);
		assert!(seg.buffer[0].timestamp <= seg.buffer[1].timestamp);
	}

	#[tokio::test]
	#[serial]
	async fn merge_segments_leaves_no_tmp_files() {
		let (ctx, _dir) = prepare_test_ctx().await;
		let now = Utc::now();

		create_segment(
			&ctx,
			LogEntry {
				timestamp: now,
				level: LogLevel::Info,
				msg: "one".into(),
				..Default::default()
			},
		)
		.await;
		create_segment(
			&ctx,
			LogEntry {
				timestamp: now,
				level: LogLevel::Info,
				msg: "two".into(),
				..Default::default()
			},
		)
		.await;

		merge_once(&ctx).await.unwrap();

		let entries: Vec<_> = fs::read_dir(ctx.logs_dir()).unwrap().collect();
		assert_eq!(entries.len(), 1);
		let name = entries[0].as_ref().unwrap().file_name();
		let fname = name.to_string_lossy();
		assert!(fname.ends_with(".log"));
	}

	#[tokio::test]
	#[serial]
	async fn merge_segments_skips_non_overlapping_segments() {
		let (ctx, _dir) = prepare_test_ctx().await;
		let now = Utc::now();

		let id1 = create_segment(
			&ctx,
			LogEntry {
				timestamp: now - chrono::Duration::seconds(5),
				level: LogLevel::Info,
				msg: "one".into(),
				..Default::default()
			},
		)
		.await;

		let id2 = create_segment(
			&ctx,
			LogEntry {
				timestamp: now + chrono::Duration::seconds(5),
				level: LogLevel::Info,
				msg: "two".into(),
				..Default::default()
			},
		)
		.await;

		merge_once(&ctx).await.unwrap();

		let metas = ctx
			.db
			.find_segments(&GetSegmentsQuery {
				start: None,
				end: None,
				count: None,
				sort: Some(SortDir::Asc),
			})
			.await
			.unwrap();

		assert_eq!(metas.len(), 2);
		assert!(ctx.logs_dir().join(format!("{}.log", id1)).exists());
		assert!(ctx.logs_dir().join(format!("{}.log", id2)).exists());
	}

	fn meta(
		id: u32,
		first: chrono::DateTime<Utc>,
		last: chrono::DateTime<Utc>,
		count: u64,
	) -> SegmentMeta {
		SegmentMeta {
			id,
			first_timestamp: first,
			last_timestamp: last,
			original_size: 0,
			compressed_size: 0,
			logs_count: count,
			created_at: Utc::now(),
		}
	}

	#[test]
	fn windows_for_merge_groups_overlaps() {
		let now = Utc::now();
		let metas = vec![
			meta(1, now, now + Duration::seconds(5), 10),
			meta(
				2,
				now + Duration::seconds(3),
				now + Duration::seconds(7),
				10,
			),
			meta(
				3,
				now + Duration::seconds(10),
				now + Duration::seconds(12),
				10,
			),
		];
		assert_eq!(windows_for_merge(&metas), vec![(0, 2)]);
	}

	#[test]
	fn windows_for_merge_respects_target_logs() {
		let now = Utc::now();
		let half = (TARGET_LOGS / 2) as u64;
		let metas = vec![
			meta(1, now, now + Duration::seconds(1), half),
			meta(
				2,
				now + Duration::milliseconds(500),
				now + Duration::seconds(2),
				half,
			),
			meta(
				3,
				now + Duration::seconds(2),
				now + Duration::seconds(3),
				half,
			),
			meta(
				4,
				now + Duration::seconds(2),
				now + Duration::seconds(4),
				half,
			),
		];
		assert_eq!(windows_for_merge(&metas), vec![(0, 2), (2, 4)]);
	}

	#[test]
	fn windows_for_merge_no_overlap() {
		let now = Utc::now();
		let metas = vec![
			meta(1, now, now + Duration::seconds(1), 1),
			meta(2, now + Duration::seconds(2), now + Duration::seconds(3), 1),
		];
		assert!(windows_for_merge(&metas).is_empty());
	}

	#[test]
	fn windows_for_merge_chain_overlaps() {
		let now = Utc::now();
		let metas = vec![
			meta(1, now, now + Duration::seconds(2), 1),
			meta(2, now + Duration::seconds(1), now + Duration::seconds(3), 1),
			meta(3, now + Duration::seconds(2), now + Duration::seconds(4), 1),
		];
		assert_eq!(windows_for_merge(&metas), vec![(0, 3)]);
	}
}
