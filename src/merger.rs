use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::context::Context;
use crate::db::NewSegmentArgs;
use crate::segment::LogSegment;
use puppylog::{LogEntry, Prop};

pub const TARGET_SEGMENT_SIZE: usize = 300_000;

pub struct DeviceMerger {
	ctx: Arc<Context>,
	buffers: HashMap<String, Vec<LogEntry>>, // deviceId -> buffered logs
}

impl DeviceMerger {
	pub fn new(ctx: Arc<Context>) -> Self {
		Self {
			ctx,
			buffers: HashMap::new(),
		}
	}

	async fn flush_device(&mut self, device_id: &str) -> anyhow::Result<()> {
		if let Some(mut logs) = self.buffers.remove(device_id) {
			if logs.is_empty() {
				return Ok(());
			}
			logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
			let first = logs.first().unwrap().timestamp;
			let last = logs.last().unwrap().timestamp;
			let mut seg = LogSegment { buffer: logs };
			let mut buf = Vec::new();
			seg.serialize(&mut buf);
			let orig_size = buf.len();
			let compressed = zstd::encode_all(std::io::Cursor::new(buf), 0)?;
			let comp_size = compressed.len();
			let segment_id = self
				.ctx
				.db
				.new_segment(NewSegmentArgs {
					device_id: Some(device_id.to_string()),
					first_timestamp: first,
					last_timestamp: last,
					original_size: orig_size,
					compressed_size: comp_size,
					logs_count: seg.buffer.len() as u64,
				})
				.await?;
			let mut unique = HashSet::new();
			for log in &seg.buffer {
				for p in &log.props {
					unique.insert(p.clone());
				}
				unique.insert(Prop {
					key: "level".into(),
					value: log.level.to_string(),
				});
			}
			self.ctx
				.db
				.upsert_segment_props(segment_id, unique.iter())
				.await?;
			let path = self.ctx.logs_path().join(format!("{}.log", segment_id));
			std::fs::write(path, compressed)?;
		}
		Ok(())
	}

	pub async fn run_once(&mut self) -> anyhow::Result<bool> {
		use tokio::fs::remove_file;

		let segments = self.ctx.db.find_segments_without_device(None).await?;
		let mut processed = false;
		for seg in segments {
			let path = self.ctx.logs_path().join(format!("{}.log", seg.id));
			let file = match std::fs::File::open(&path) {
				Ok(f) => f,
				Err(_) => continue,
			};
			let mut decoder = zstd::Decoder::new(file)?;
			let log_seg = LogSegment::parse(&mut decoder);
			for log in log_seg.buffer {
				if let Some(prop) = log.props.iter().find(|p| p.key == "deviceId").cloned() {
					let buf = self.buffers.entry(prop.value.clone()).or_default();
					buf.push(log);
					if buf.len() >= TARGET_SEGMENT_SIZE {
						self.flush_device(&prop.value).await?;
					}
				}
			}
			self.ctx.db.delete_segment(seg.id).await?;
			let _ = remove_file(path).await;
			processed = true;
		}
		let keys: Vec<String> = self.buffers.keys().cloned().collect();
		for k in keys {
			if processed || self.buffers.get(&k).map_or(0, |v| v.len()) >= TARGET_SEGMENT_SIZE {
				self.flush_device(&k).await?;
			}
		}
		Ok(processed)
	}
}

pub async fn run_device_merger(ctx: Arc<Context>) {
	let mut merger = DeviceMerger::new(ctx);
	loop {
		let processed = merger.run_once().await.unwrap_or(false);
		if !processed {
			tokio::time::sleep(Duration::from_secs(5)).await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use puppylog::{LogEntry, LogLevel};
	use tempfile::tempdir;
	use tokio::fs;

	async fn prepare_ctx() -> (Arc<Context>, tempfile::TempDir) {
		let dir = tempdir().unwrap();
		let logs_path = dir.path().join("logs");
		fs::create_dir_all(&logs_path).await.unwrap();
		let ctx = Context::new(logs_path).await;
		(Arc::new(ctx), dir)
	}

	#[tokio::test]
	async fn merge_single_segment() {
		let (ctx, _dir) = prepare_ctx().await;
		let ts = Utc::now();
		let log = LogEntry {
			timestamp: ts,
			level: LogLevel::Info,
			props: vec![Prop {
				key: "deviceId".into(),
				value: "dev1".into(),
			}],
			msg: "msg".into(),
			..Default::default()
		};
		let mut seg = LogSegment::new();
		seg.add_log_entry(log.clone());
		seg.sort();
		let mut buff = Vec::new();
		seg.serialize(&mut buff);
		let orig = buff.len();
		let comp = zstd::encode_all(std::io::Cursor::new(buff), 0).unwrap();
		let comp_size = comp.len();
		let seg_id = ctx
			.db
			.new_segment(NewSegmentArgs {
				device_id: None,
				first_timestamp: ts,
				last_timestamp: ts,
				original_size: orig,
				compressed_size: comp_size,
				logs_count: 1,
			})
			.await
			.unwrap();
		std::fs::write(ctx.logs_path().join(format!("{}.log", seg_id)), comp).unwrap();

		let mut merger = DeviceMerger::new(ctx.clone());
		assert!(merger.run_once().await.unwrap());

		let segs = ctx
			.db
			.find_segments(&crate::types::GetSegmentsQuery::default())
			.await
			.unwrap();
		assert_eq!(segs.len(), 1);
		assert_eq!(segs[0].device_id.as_deref(), Some("dev1"));
	}
}
