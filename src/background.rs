use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::fs::{create_dir_all, metadata, read_dir, remove_file, File};
use tokio::io::AsyncReadExt;
use tokio::time::sleep;

use crate::utility::{available_space, disk_usage};

use crate::slack;

use crate::config::upload_path;
use crate::context::Context;
use crate::types::{GetSegmentsQuery, SortDir};
use puppylog::{LogEntry, LogentryDeserializerError};

const DISK_LOW: u64 = 1_000_000_000; // 1GB
const DISK_OK: u64 = 2_000_000_000; // 2GB

async fn cleanup_old_segments(ctx: &Context, min_free_ratio: f64) {
	if let Some((mut free, total)) = disk_usage(ctx.logs_path()) {
		let start_free = free;
		let target = (total as f64 * min_free_ratio) as u64;
		let mut removed = 0u64;
		while free < target {
			let segs = ctx
				.db
				.find_segments(&GetSegmentsQuery {
					start: None,
					end: None,
					device_ids: None,
					count: Some(1),
					sort: Some(SortDir::Asc),
				})
				.await
				.unwrap_or_default();
			if segs.is_empty() {
				break;
			}
			let seg = &segs[0];
			let path = ctx.logs_path().join(format!("{}.log", seg.id));
			log::warn!("deleting old segment {}", path.display());
			let _ = remove_file(&path).await;
			ctx.db.delete_segment(seg.id).await.ok();
			removed += 1;
			free = disk_usage(ctx.logs_path()).map(|(f, _)| f).unwrap_or(free);
		}
		if removed > 0 {
			if let Some((new_free, _)) = disk_usage(ctx.logs_path()) {
				let freed = new_free.saturating_sub(start_free);
				log::info!(
					"Deleted {removed} old segment{pl} freeing {:.1} MB",
					freed as f64 / 1_048_576.0,
					pl = if removed == 1 { "" } else { "s" },
				);
			}
		}
	}
}

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
		if let Some((f, total)) = disk_usage(ctx.logs_path()) {
			if f * 10 < total {
				cleanup_old_segments(&ctx, 0.05).await;
			}
		}
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
