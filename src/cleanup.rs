use std::sync::Arc;
use std::time::Duration;

use tokio::fs::{create_dir_all, remove_file};
use tokio::time::sleep;

use crate::config::upload_path;
use crate::context::Context;
use crate::slack;
use crate::types::{GetSegmentsQuery, SortDir};
use crate::utility::disk_usage;

// Deletes oldest segments until free space reaches the given ratio.
pub async fn cleanup_old_segments(ctx: &Context) {
	let count = std::env::var("CLEANUP_DELETE_COUNT")
		.ok()
		.and_then(|v| v.parse::<usize>().ok())
		.unwrap_or(20);

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
		return;
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
		let (f, total) = match disk_usage(&upload_dir) {
			Some((f, t)) => (f, t),
			None => {
				log::error!("cannot get disk usage for {}", upload_dir.display());
				continue;
			}
		};

		let free_p = f as f64 / total as f64;
		let msg = format!(
			"disk usage: {} MB free of {} MB ({:.1}%)",
			f / 1_048_576,
			total / 1_048_576,
			free_p * 100.0
		);
		log::info!("{}", &msg);

		if free_p > 0.1 {
			low_disk = false;
			sleep(Duration::from_secs(300)).await;
			continue;
		}

		if !low_disk {
			slack::notify(&msg).await;
			low_disk = true;
		}

		ctx.force_flush().await;
		cleanup_old_segments(&ctx).await;
		sleep(Duration::from_millis(500)).await;
	}
}
