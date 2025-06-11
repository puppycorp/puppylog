use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::fs::{create_dir_all, metadata, read_dir, remove_file, File};
use tokio::io::AsyncReadExt;
use tokio::time::sleep;

use crate::utility::available_space;

use crate::slack;

use crate::config::upload_path;
use crate::context::Context;
use puppylog::{LogEntry, LogentryDeserializerError};

const DISK_LOW: u64 = 1_000_000_000; // 1GB
const DISK_OK: u64 = 2_000_000_000; // 2GB

// Background task that imports *.ready log files into the DB.
pub async fn process_log_uploads(ctx: Arc<Context>) {
	let upload_dir = upload_path();
	if !upload_dir.exists() {
		match create_dir_all(upload_dir.clone()).await {
			Ok(_) => log::info!("created upload directory {:?}", upload_dir),
			Err(e) => {
				log::error!("cannot create {}: {}", upload_dir.display(), e);
				return;
			}
		}
	}
	let mut low_disk = false;

	loop {
		let free = available_space(&upload_dir);
		if free < DISK_LOW {
			if !low_disk {
				slack::notify(&format!(
					"Disk space running low: {} MB left",
					free / 1_048_576
				))
				.await;
				low_disk = true;
			}
		} else if free > DISK_OK {
			low_disk = false;
		}
		let mut dir = match read_dir(&upload_dir).await {
			Ok(d) => d,
			Err(e) => {
				log::error!("cannot read {}: {}", upload_dir.display(), e);
				sleep(Duration::from_secs(5)).await;
				continue;
			}
		};
		let mut buf = Vec::new();
		let mut log_entries = Vec::new();
		let timer = Instant::now();
		let mut processed_loglines = 0;

		while let Ok(Some(entry)) = dir.next_entry().await {
			let path = entry.path();
			// Clean up stale .part files (interrupted uploads older than 15 min)
			if path.extension().and_then(|e| e.to_str()) == Some("part") {
				if let Ok(meta) = metadata(&path).await {
					if let Ok(modified) = meta.modified() {
						if modified.elapsed().unwrap_or(Duration::ZERO) > Duration::from_secs(900) {
							log::warn!("removing stale .part file {}", path.display());
							let _ = remove_file(&path).await;
						}
					}
				}
				continue;
			}

			if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("ready") {
				continue;
			}

			match File::open(&path).await {
				Ok(mut file) => {
					buf.clear();
					if let Err(e) = file.read_to_end(&mut buf).await {
						log::error!("failed to read {}: {}", path.display(), e);
						continue;
					}

					let mut ptr = 0;
					log_entries.clear();
					loop {
						if processed_loglines % 1_000_000 == 0 {
							let elapsed = timer.elapsed();
							let rate = processed_loglines as f64 / elapsed.as_secs_f64();
							log::info!(
								"[{}] processed in {:.2?} seconds at {:.2} loglines/s",
								processed_loglines,
								elapsed,
								rate
							);
						}
						processed_loglines += 1;
						match LogEntry::fast_deserialize(&buf, &mut ptr) {
							Ok(log_entry) => log_entries.push(log_entry),
							Err(LogentryDeserializerError::NotEnoughData) => break,
							Err(err) => log::error!("Error deserializing log entry: {:?}", err),
						}
					}

					ctx.save_logs(&log_entries).await;
					let log_count = log_entries.len();
					let total_bytes = buf.len();

					if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
						if let Some((device_id, _rest)) = stem.split_once('-') {
							if let Err(e) = ctx
								.db
								.update_device_stats(device_id, total_bytes, log_count)
								.await
							{
								log::warn!("update_device_stats failed for {}: {}", device_id, e);
							}
						}
					}

					if let Err(e) = remove_file(&path).await {
						log::warn!("failed to delete {}: {}", path.display(), e);
					}
				}
				Err(e) => {
					log::error!("cannot open {}: {}", path.display(), e);
				}
			}
		}

		if processed_loglines > 0 {
			let elapsed = timer.elapsed();
			let rate = processed_loglines as f64 / elapsed.as_secs_f64();
			log::info!(
				"processed {} log entries in {:.2} seconds at {:.2} entries/s",
				processed_loglines,
				elapsed.as_secs_f64(),
				rate
			);
		}

		sleep(Duration::from_secs(2)).await;
	}
}

use rand::Rng;
use std::collections::HashSet;
use std::io::Cursor;

use crate::config::log_path;
use crate::db::NewSegmentArgs;
use crate::segment::LogSegment;
use crate::types::{GetSegmentsQuery, SortDir};
use puppylog::Prop;

const TARGET_LOGS: usize = 100_000;

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

	let log_dir = log_path();
	let mut merged_logs = Vec::new();
	let mut to_delete = Vec::new();

	for meta in segments {
		let path = log_dir.join(format!("{}.log", meta.id));
		let compressed = tokio::fs::read(&path).await?;
		let decoded = zstd::decode_all(Cursor::new(compressed))?;
		let mut cursor = Cursor::new(decoded);
		let seg = LogSegment::parse(&mut cursor);
		merged_logs.extend(seg.buffer.into_iter());
		to_delete.push(meta.id);
		if merged_logs.len() >= TARGET_LOGS {
			write_segment(ctx, &merged_logs, &to_delete, &log_dir).await?;
			merged_logs.clear();
			to_delete.clear();
		}
	}
	if to_delete.len() > 1 {
		write_segment(ctx, &merged_logs, &to_delete, &log_dir).await?;
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
	use chrono::Utc;
	use puppylog::{LogEntry, LogLevel, Prop};
	use std::fs;
	use tempfile::tempdir;

	async fn prepare_test_ctx() -> (Arc<Context>, tempfile::TempDir) {
		let dir = tempdir().unwrap();
		let logs_dir = dir.path().join("logs");
		fs::create_dir_all(&logs_dir).unwrap();
		std::env::set_var("LOG_PATH", &logs_dir);
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
		tokio::fs::write(log_path().join(format!("{}.log", id)), compressed)
			.await
			.unwrap();
		id
	}

	#[tokio::test]
	async fn merge_segments_combines_old_segments() {
		let (ctx, _dir) = prepare_test_ctx().await;
		let now = Utc::now();

		let id1 = create_segment(
			&ctx,
			LogEntry {
				timestamp: now - chrono::Duration::seconds(1),
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
		assert!(!log_path().join(format!("{}.log", id1)).exists());
		assert!(!log_path().join(format!("{}.log", id2)).exists());
		assert!(log_path().join(format!("{}.log", merged_id)).exists());

		let compressed = fs::read(log_path().join(format!("{}.log", merged_id))).unwrap();
		let decoded = zstd::decode_all(Cursor::new(compressed)).unwrap();
		let mut cursor = Cursor::new(decoded);
		let seg = LogSegment::parse(&mut cursor);
		assert_eq!(seg.buffer.len(), 2);
		assert!(seg.buffer[0].timestamp <= seg.buffer[1].timestamp);
	}
}
