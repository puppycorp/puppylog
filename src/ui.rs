use crate::context::Context;
use crate::db::{Device, SegmentsMetadata};
use crate::server_info::{fetch_server_info, ServerInfo};
use chrono::{DateTime, Utc};
use log::error;
use std::sync::{Arc, Mutex};
use wgui::wui::runtime::{Component, Ctx};
use wgui::{wgui_controller, WguiModel};

const DEVICE_ROW_LIMIT: usize = 12;

pub struct UiContext {
	pub app: Arc<Context>,
	pub snapshot: Mutex<UiSnapshot>,
}

impl UiContext {
	pub fn new(app: Arc<Context>) -> Self {
		Self {
			app,
			snapshot: Mutex::new(UiSnapshot::empty()),
		}
	}
}

pub struct Ui {
	ctx: Arc<Ctx<UiContext, ()>>,
}

#[derive(Debug, Clone, WguiModel)]
pub struct UiStatLine {
	pub line: String,
}

#[derive(Debug, Clone, WguiModel)]
pub struct UiDeviceRow {
	pub id: String,
	pub logs_count: String,
	pub last_upload_at: String,
	pub send_logs: String,
}

#[derive(Debug, Clone, WguiModel)]
pub struct UiSnapshot {
	pub storage_stats: Vec<UiStatLine>,
	pub segment_stats: Vec<UiStatLine>,
	pub segment_stats_available: bool,
	pub devices: Vec<UiDeviceRow>,
	pub has_devices: bool,
	pub has_error: bool,
	pub last_error: String,
	pub last_updated: String,
}

impl UiSnapshot {
	pub fn empty() -> Self {
		Self {
			storage_stats: Vec::new(),
			segment_stats: Vec::new(),
			segment_stats_available: false,
			devices: Vec::new(),
			has_devices: false,
			has_error: false,
			last_error: String::new(),
			last_updated: String::new(),
		}
	}

	pub async fn capture(ctx: &Arc<Context>) -> Self {
		let server_info = fetch_server_info().await;

		let mut errors = Vec::new();
		let segments_metadata = match ctx.db.fetch_segments_metadata().await {
			Ok(meta) => Some(meta),
			Err(err) => {
				error!("failed to read segment metadata: {}", err);
				errors.push(format!("segment stats unavailable ({})", err));
				None
			}
		};
		let devices = match ctx.db.get_devices().await {
			Ok(devices) => devices,
			Err(err) => {
				error!("failed to load devices: {}", err);
				errors.push(format!("devices unavailable ({})", err));
				Vec::new()
			}
		};

		let last_error = if errors.is_empty() {
			String::new()
		} else {
			errors.join(" | ")
		};

		Self::from_parts(
			server_info,
			segments_metadata,
			devices,
			Utc::now(),
			last_error,
		)
	}

	fn from_parts(
		server_info: ServerInfo,
		segments_metadata: Option<SegmentsMetadata>,
		devices: Vec<Device>,
		last_updated: DateTime<Utc>,
		last_error: String,
	) -> Self {
		let storage_stats = vec![
			UiStatLine {
				line: format!(
					"Used: {} ({:.1}%)",
					format_bytes(server_info.used_bytes),
					server_info.used_percent,
				),
			},
			UiStatLine {
				line: format!("Free: {}", format_bytes(server_info.free_bytes)),
			},
			UiStatLine {
				line: format!(
					"Uploads: {} files, {}",
					server_info.upload_files_count,
					format_bytes(server_info.upload_bytes)
				),
			},
		];

		let (segment_stats_available, segment_stats) = if let Some(meta) = segments_metadata {
			(
				true,
				vec![
					UiStatLine {
						line: format!("Count: {}", meta.segment_count),
					},
					UiStatLine {
						line: format!("Original size: {}", format_bytes(meta.original_size)),
					},
					UiStatLine {
						line: format!("Compressed size: {}", format_bytes(meta.compressed_size)),
					},
					UiStatLine {
						line: format!("Total logs: {}", meta.logs_count),
					},
				],
			)
		} else {
			(false, Vec::new())
		};

		let devices: Vec<UiDeviceRow> = devices
			.into_iter()
			.take(DEVICE_ROW_LIMIT)
			.map(|device| UiDeviceRow {
				id: device.id,
				logs_count: device.logs_count.to_string(),
				last_upload_at: device
					.last_upload_at
					.map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
					.unwrap_or_else(|| "never".to_string()),
				send_logs: if device.send_logs {
					"enabled".to_string()
				} else {
					"disabled".to_string()
				},
			})
			.collect();

		Self {
			storage_stats,
			segment_stats,
			segment_stats_available,
			has_devices: !devices.is_empty(),
			devices,
			has_error: !last_error.is_empty(),
			last_error,
			last_updated: format!("Last updated: {}", last_updated.format("%Y-%m-%d %H:%M:%S")),
		}
	}
}

impl Ui {
	pub async fn new(ctx: Arc<Ctx<UiContext>>) -> Self {
		let mut ui = Self { ctx };
		ui.refresh().await;
		ui
	}
}

#[wgui_controller]
impl Ui {
	pub fn state(&self) -> UiSnapshot {
		self.ctx.state.snapshot.lock().unwrap().clone()
	}

	pub async fn refresh(&mut self) {
		let snapshot = UiSnapshot::capture(&self.ctx.state.app).await;
		*self.ctx.state.snapshot.lock().unwrap() = snapshot;
	}
}

#[wgui::wui::runtime::async_trait]
impl Component for Ui {
	type Context = UiContext;
	type Db = ();
	type Model = UiSnapshot;

	async fn mount(ctx: Arc<Ctx<UiContext, ()>>) -> Self {
		Self::new(ctx).await
	}

	fn render(&self, _ctx: &Ctx<UiContext, ()>) -> Self::Model {
		self.state()
	}

	fn unmount(self, _ctx: Arc<Ctx<UiContext, ()>>) {}
}

fn format_bytes(bytes: u64) -> String {
	const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
	let mut value = bytes as f64;
	let mut unit = 0;
	while value >= 1024.0 && unit < UNITS.len() - 1 {
		value /= 1024.0;
		unit += 1;
	}
	format!("{:.2} {}", value, UNITS[unit])
}
