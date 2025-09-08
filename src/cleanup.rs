use std::sync::Arc;
use std::time::Duration;

use tokio::fs::{create_dir_all, remove_file};
use tokio::time::sleep;

use crate::config::upload_path;
use crate::context::Context;
use crate::slack;
use crate::types::{GetSegmentsQuery, SortDir};
use crate::utility::{available_space, disk_usage};

const DISK_LOW: u64 = 1_000_000_000; // 1GB
const DISK_OK: u64 = 2_000_000_000; // 2GB

// Deletes oldest segments until free space reaches the given ratio.
pub async fn cleanup_old_segments(ctx: &Context, min_free_ratio: f64) {
	let count = std::env::var("CLEANUP_DELETE_COUNT")
		.ok()
		.and_then(|v| v.parse::<usize>().ok())
		.unwrap_or(20);

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
					count: Some(count),
					sort: Some(SortDir::Asc),
				})
				.await
				.unwrap_or_default();
			if segs.is_empty() {
				break;
			}
			for seg in segs {
				let path = ctx.logs_path().join(format!("{}.log", seg.id));
				log::warn!("deleting old segment {}", path.display());
				if let Err(err) = remove_file(&path).await {
					log::error!("failed to delete log file {}: {}", path.display(), err);
				}
				if let Err(err) = ctx.db.delete_segment(seg.id).await {
					log::error!("failed to delete segment {} from DB: {}", seg.id, err);
				}
				removed += 1;
				free = disk_usage(ctx.logs_path()).map(|(f, _)| f).unwrap_or(free);
			}
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

// Monitors disk space and triggers cleanup when low
pub async fn run_disk_space_monitor(ctx: Arc<Context>) {
	let upload_dir = upload_path();
	if !upload_dir.exists() {
		if let Err(e) = create_dir_all(upload_dir.clone()).await {
			log::error!("cannot create {}: {}", upload_dir.display(), e);
			return;
		}
	}
	let mut low_disk = false;
	loop {
		if let Some((f, total)) = disk_usage(ctx.logs_path()) {
			// If free space < 10% of total, first try to flush WAL,
			// then delete old segments until at least 15% free.
			if f * 10 < total {
				log::info!(
					"Low disk space: {} MB free of {} MB total",
					f / 1_048_576,
					total / 1_048_576
				);
				ctx.force_flush().await;
				cleanup_old_segments(&ctx, 0.15).await;
			}
		}

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

		sleep(Duration::from_secs(2)).await;
	}
}
