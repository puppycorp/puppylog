use crate::context::Context;
use crate::db::{Device, SegmentsMetadata};
use crate::server_info::{fetch_server_info, ServerInfo};
use chrono::{DateTime, Utc};
use log::error;
use std::sync::Arc;
use wgui::*;

pub const REFRESH_BUTTON_ID: u32 = 1;
const DEVICE_ROW_LIMIT: usize = 12;

pub struct UiSnapshot {
	pub server_info: ServerInfo,
	pub segments_metadata: Option<SegmentsMetadata>,
	pub devices: Vec<Device>,
	pub last_updated: DateTime<Utc>,
	pub last_error: Option<String>,
}

impl UiSnapshot {
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
			None
		} else {
			Some(errors.join(" | "))
		};

		UiSnapshot {
			server_info,
			segments_metadata,
			devices,
			last_updated: Utc::now(),
			last_error,
		}
	}
}

pub fn render(snapshot: &UiSnapshot) -> Item {
	let header = hstack([
		text("PuppyLog").grow(1).text_align("left"),
		button("Refresh").id(REFRESH_BUTTON_ID),
	])
	.spacing(12)
	.padding(12)
	.border("1px solid #dcdcdc")
	.background_color("#f7f7f7");

	let storage_card = vstack([
		text("Storage").text_align("center"),
		text(&format!(
			"Used: {} ({:.1}%)",
			format_bytes(snapshot.server_info.used_bytes),
			snapshot.server_info.used_percent,
		))
		.text_align("center"),
		text(&format!(
			"Free: {}",
			format_bytes(snapshot.server_info.free_bytes)
		))
		.text_align("center"),
		text(&format!(
			"Uploads: {} files, {}",
			snapshot.server_info.upload_files_count,
			format_bytes(snapshot.server_info.upload_bytes)
		))
		.text_align("center"),
	])
	.spacing(8)
	.padding(12)
	.border("1px solid #dddddd")
	.background_color("#ffffff")
	.grow(1);

	let segments_card = if let Some(meta) = &snapshot.segments_metadata {
		vstack([
			text("Segments").text_align("center"),
			text(&format!("Count: {}", meta.segment_count)).text_align("center"),
			text(&format!(
				"Original size: {}",
				format_bytes(meta.original_size)
			))
			.text_align("center"),
			text(&format!(
				"Compressed size: {}",
				format_bytes(meta.compressed_size)
			))
			.text_align("center"),
			text(&format!("Total logs: {}", meta.logs_count)).text_align("center"),
		])
		.spacing(6)
		.padding(12)
		.border("1px solid #dddddd")
		.background_color("#ffffff")
		.grow(1)
	} else {
		text("Segment stats unavailable")
			.text_align("center")
			.padding(12)
			.border("1px solid #f5c6cb")
			.background_color("#fff5f5")
			.grow(1)
	};

	let device_rows: Vec<Item> = snapshot
		.devices
		.iter()
		.take(DEVICE_ROW_LIMIT)
		.map(|device| {
			tr([
				td(text(&device.id)),
				td(text(&device.logs_count.to_string())).text_align("center"),
				td(text(
					&device
						.last_upload_at
						.map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
						.unwrap_or_else(|| "never".to_string()),
				)),
				td(text(if device.send_logs {
					"enabled"
				} else {
					"disabled"
				}))
				.text_align("center"),
			])
		})
		.collect();

	let device_table = if device_rows.is_empty() {
		text("No devices yet")
			.padding(12)
			.border("1px solid #dddddd")
	} else {
		table([
			thead([tr([
				th(text("Device ID")),
				th(text("Logs")),
				th(text("Last upload")),
				th(text("Send logs")),
			])]),
			tbody(device_rows),
		])
		.border("1px solid #dddddd")
		.wrap(true)
	};

	let mut layout = vec![
		header,
		hstack([storage_card, segments_card]).spacing(12).wrap(true),
		text("Devices").margin_top(8).text_align("left"),
		device_table,
	];

	if let Some(error) = &snapshot.last_error {
		layout.push(
			text(error)
				.padding(10)
				.border("1px solid #f5c6cb")
				.background_color("#fff5f5"),
		);
	}

	layout.push(
		text(&format!(
			"Last updated: {}",
			snapshot.last_updated.format("%Y-%m-%d %H:%M:%S")
		))
		.text_align("right")
		.margin_top(8),
	);

	vstack(layout)
		.spacing(16)
		.padding(20)
		.background_color("#f9fbff")
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
