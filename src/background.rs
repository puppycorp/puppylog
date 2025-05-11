use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::fs::{create_dir_all, metadata, read_dir, remove_file, File};
use tokio::io::AsyncReadExt;
use tokio::time::sleep;

use crate::config::upload_path;
use crate::context::Context;
use puppylog::{LogEntry, LogentryDeserializerError};

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

	loop {
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
							log::info!("[{}] processed in {:.2?} seconds at {:.2} loglines/s", processed_loglines, elapsed, rate);
						}
						processed_loglines += 1;
						match LogEntry::fast_deserialize(&buf, &mut ptr) {
							Ok(log_entry) => log_entries.push(log_entry),
							Err(LogentryDeserializerError::NotEnoughData) => break,
							Err(err) => log::error!("Error deserializing log entry: {:?}", err)
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
			log::info!("processed {} log entries in {:.2} seconds at {:.2} entries/s", processed_loglines, elapsed.as_secs_f64(), rate);
		}

		sleep(Duration::from_secs(2)).await;
	}
}