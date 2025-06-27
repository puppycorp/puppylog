use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use crate::context::Context;
use crate::db::NewSegmentArgs;
use crate::dev_segment_merger::TARGET_SEGMENT_SIZE;
use crate::segment::LogSegment;
use crate::types::{GetSegmentsQuery, SortDir};
use puppylog::{LogEntry, Prop};
use tokio::fs::remove_file;
use zstd::Encoder;

pub struct DeviceSegmentCompactor {
	ctx: Arc<Context>,
}

impl DeviceSegmentCompactor {
	pub fn new(ctx: Arc<Context>) -> Self {
		Self { ctx }
	}

	async fn persist_segment(&self, device_id: &str, logs: Vec<LogEntry>) -> anyhow::Result<()> {
		if logs.is_empty() {
			return Ok(());
		}
		let mut logs = logs;
		logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
		let first = logs.first().unwrap().timestamp;
		let last = logs.last().unwrap().timestamp;
		let seg = LogSegment { buffer: logs };

		let mut buf = Vec::new();
		seg.serialize(&mut buf);
		let orig_size = buf.len();

		let mut encoder = Encoder::new(Vec::new(), 14)?;
		encoder.multithread(num_cpus::get() as u32)?;
		encoder.write_all(&buf)?;
		let compressed = encoder.finish()?;
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
			unique.extend(log.props.iter().cloned());
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
		tokio::fs::write(path, compressed).await?;

		log::info!(
			"created compacted segment {} for device {} ({} logs)",
			segment_id,
			device_id,
			seg.buffer.len()
		);

		Ok(())
	}

	pub async fn run_once(&self) -> anyhow::Result<bool> {
		let segments = self
			.ctx
			.db
			.find_segments(&GetSegmentsQuery {
				start: None,
				end: None,
				device_ids: None,
				count: None,
				sort: Some(SortDir::Asc),
			})
			.await?;

		let mut by_device: HashMap<String, Vec<_>> = HashMap::new();
		for seg in segments {
			if let Some(dev) = seg.device_id.clone() {
				if seg.logs_count < TARGET_SEGMENT_SIZE as u64 {
					by_device.entry(dev).or_default().push(seg);
				}
			}
		}

		log::info!(
			"compactor found {} devices with small segments",
			by_device.len()
		);

		let mut processed = false;

		for (device, mut segs) in by_device {
			if segs.len() < 2 {
				continue;
			}
			log::info!("compacting {} segments for device {}", segs.len(), device);
			segs.sort_by_key(|s| s.first_timestamp);
			let mut buffer: Vec<LogEntry> = Vec::new();
			let mut to_delete = Vec::new();
			for seg in segs {
				let path = self.ctx.logs_path().join(format!("{}.log", seg.id));
				let file = match std::fs::File::open(&path) {
					Ok(f) => f,
					Err(err) => {
						log::warn!(
							"cannot open {} for segment {}: {}",
							path.display(),
							seg.id,
							err
						);
						self.ctx.db.delete_segment(seg.id).await?;
						let _ = remove_file(&path).await;
						continue;
					}
				};
				let mut decoder = zstd::Decoder::new(file)?;
				let log_seg = LogSegment::parse(&mut decoder);
				buffer.extend(log_seg.buffer);
				to_delete.push((seg.id, path));

				while buffer.len() >= TARGET_SEGMENT_SIZE {
					let logs: Vec<LogEntry> = buffer.drain(..TARGET_SEGMENT_SIZE).collect();
					self.persist_segment(&device, logs).await?;
					processed = true;
				}
			}

			if !buffer.is_empty() {
				self.persist_segment(&device, buffer.clone()).await?;
				buffer.clear();
				processed = true;
			}

			for (id, path) in to_delete {
				self.ctx.db.delete_segment(id).await?;
				let _ = remove_file(path).await;
			}
			log::info!("device {} compacted", device);
		}

		Ok(processed)
	}
}

pub async fn run_device_segment_compactor(ctx: Arc<Context>) {
	let compactor = DeviceSegmentCompactor::new(ctx);
	loop {
		if let Err(err) = compactor.run_once().await {
			log::error!("segment compaction failed: {}", err);
		}
		tokio::time::sleep(Duration::from_secs(30)).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use puppylog::{LogLevel, Prop};
	use puppylog_server::segment::compress_segment;
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
	async fn merge_small_segments_sorted() {
		let (ctx, _dir) = prepare_ctx().await;
		let ts1 = Utc::now();
		let ts2 = ts1 + chrono::Duration::seconds(1);
		let ts3 = ts2 + chrono::Duration::seconds(1);

		let times = [ts3, ts1, ts2];
		for ts in times {
			let mut seg = LogSegment::new();
			seg.add_log_entry(LogEntry {
				timestamp: ts,
				level: LogLevel::Info,
				props: vec![Prop {
					key: "deviceId".into(),
					value: "dev".into(),
				}],
				msg: format!("log-{ts}"),
				..Default::default()
			});
			seg.sort();
			let mut buf = Vec::new();
			seg.serialize(&mut buf);
			let orig = buf.len();
			let comp = compress_segment(&buf).unwrap();
			let comp_size = comp.len();
			let seg_id = ctx
				.db
				.new_segment(NewSegmentArgs {
					device_id: Some("dev".into()),
					first_timestamp: ts,
					last_timestamp: ts,
					original_size: orig,
					compressed_size: comp_size,
					logs_count: 1,
				})
				.await
				.unwrap();
			std::fs::write(ctx.logs_path().join(format!("{}.log", seg_id)), comp).unwrap();
		}

		let compactor = DeviceSegmentCompactor::new(ctx.clone());
		assert!(compactor.run_once().await.unwrap());

		let segs = ctx
			.db
			.find_segments(&GetSegmentsQuery::default())
			.await
			.unwrap();
		assert_eq!(segs.len(), 1);
		assert_eq!(segs[0].logs_count, 3);
		let path = ctx.logs_path().join(format!("{}.log", segs[0].id));
		let file = std::fs::File::open(&path).unwrap();
		let mut decoder = zstd::Decoder::new(file).unwrap();
		let seg = LogSegment::parse(&mut decoder);
		let mut ts: Vec<_> = seg.buffer.iter().map(|l| l.timestamp).collect();
		let mut sorted = ts.clone();
		sorted.sort();
		assert_eq!(ts, sorted);
	}
}
