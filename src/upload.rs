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
			// Clean up stale .part files (interrupted uploads older than 5 min)
			if path.extension().and_then(|e| e.to_str()) == Some("part") {
				if let Ok(meta) = metadata(&path).await {
					if let Ok(modified) = meta.modified() {
						if modified.elapsed().unwrap_or(Duration::ZERO) > Duration::from_secs(300) {
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

			// Skip files that are too "hot" (recently modified), likely still being written.
			if let Ok(meta) = metadata(&path).await {
				if let Ok(modified) = meta.modified() {
					if modified.elapsed().unwrap_or(Duration::ZERO) < Duration::from_secs(10) {
						// Revisit on next scan.
						continue;
					}
				}
			}

			match File::open(&path).await {
				Ok(mut file) => {
					buf.clear();
					log_entries.clear();
					let mut ptr: usize = 0;
					let mut chunk = vec![0u8; 8 * 1024 * 1024]; // 8 MiB chunks
					loop {
						match file.read(&mut chunk).await {
							Ok(0) => {
								// EOF reached; fall through to parse remaining buffer below.
								break;
							}
							Ok(n) => {
								buf.extend_from_slice(&chunk[..n]);
								// Try to parse as much as we can from current buffer.
								loop {
									if processed_loglines % 1_000_000 == 0 {
										let elapsed = timer.elapsed();
										let rate =
											processed_loglines as f64 / elapsed.as_secs_f64();
										log::info!(
											"[{}] processed in {:.2?} seconds at {:.2} loglines/s",
											processed_loglines,
											elapsed,
											rate
										);
									}
									match LogEntry::fast_deserialize(&buf, &mut ptr) {
										Ok(log_entry) => {
											processed_loglines += 1;
											log_entries.push(log_entry);
										}
										Err(LogentryDeserializerError::NotEnoughData) => {
											// Retain the unconsumed tail to avoid unbounded memory usage.
											if ptr > 0 {
												buf.drain(..ptr);
												ptr = 0;
											}
											break;
										}
										Err(err) => {
											// Log and skip a byte to avoid infinite loops on corrupt data.
											log::error!("Error deserializing log entry: {:?}", err);
											ptr = ptr.saturating_add(1);
										}
									}
								}
							}
							Err(e) => {
								log::error!("failed to read {}: {}", path.display(), e);
								// On read error, skip this file for now; we'll try again later.
								continue;
							}
						}
					}

					// Final parse pass after EOF to drain remaining complete entries.
					loop {
						match LogEntry::fast_deserialize(&buf, &mut ptr) {
							Ok(log_entry) => {
								processed_loglines += 1;
								log_entries.push(log_entry);
							}
							Err(LogentryDeserializerError::NotEnoughData) => break,
							Err(err) => {
								log::error!("Error deserializing log entry at EOF: {:?}", err);
								ptr = ptr.saturating_add(1);
							}
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
