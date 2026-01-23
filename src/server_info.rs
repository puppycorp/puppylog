use crate::config::upload_path;
use crate::utility::disk_usage;
use serde::Serialize;
use tokio::fs::read_dir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
	pub free_bytes: u64,
	pub total_bytes: u64,
	pub used_bytes: u64,
	pub used_percent: f64,
	pub upload_files_count: u64,
	pub upload_bytes: u64,
}

pub async fn fetch_server_info() -> ServerInfo {
	let upload_dir = upload_path();

	let (free, total) = disk_usage(&upload_dir).unwrap_or((0, 0));
	let used = total.saturating_sub(free);
	let used_percent = if total > 0 {
		(used as f64) / (total as f64) * 100.0
	} else {
		0.0
	};

	let mut upload_files_count: u64 = 0;
	let mut upload_bytes: u64 = 0;
	if upload_dir.exists() {
		if let Ok(mut dir) = read_dir(&upload_dir).await {
			while let Ok(Some(entry)) = dir.next_entry().await {
				let path = entry.path();
				if let Ok(meta) = entry.metadata().await {
					if meta.is_file() {
						upload_files_count = upload_files_count.saturating_add(1);
						upload_bytes = upload_bytes.saturating_add(meta.len());
					}
				} else if path.is_file() {
					upload_files_count = upload_files_count.saturating_add(1);
				}
			}
		}
	}

	ServerInfo {
		free_bytes: free,
		total_bytes: total,
		used_bytes: used,
		used_percent,
		upload_files_count,
		upload_bytes,
	}
}
