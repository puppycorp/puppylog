use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::context::Context;
use crate::db::NewSegmentArgs;
use crate::segment::LogSegment;
use puppylog::{LogEntry, Prop};
use tokio::fs::remove_file;

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
		let segments = self.ctx.db.find_segments_without_device(None).await?;
		let mut processed = false;
		for seg in segments {
			let path = self.ctx.logs_path().join(format!("{}.log", seg.id));
			let file = match std::fs::File::open(&path) {
				Ok(f) => f,
				Err(_) => continue,
			};
			log::info!("process segment {} from {}", seg.id, path.display());
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

    /// Ensure that when the total buffered logs for a device do **not** reach
    /// `TARGET_SEGMENT_SIZE`, the merger still flushes them (because `processed`
    /// is true) and no data is lost.
    #[tokio::test]
    async fn small_buffer_still_persisted() {
        let (ctx, _dir) = prepare_ctx().await;
        let ts1 = chrono::Utc::now();
        let ts2 = ts1 + chrono::Duration::seconds(1);

        // Two log entries – well below TARGET_SEGMENT_SIZE.
        let logs = vec![
            LogEntry {
                timestamp: ts1,
                level: LogLevel::Info,
                props: vec![Prop {
                    key: "deviceId".into(),
                    value: "dev_small".into(),
                }],
                msg: "msg1".into(),
                ..Default::default()
            },
            LogEntry {
                timestamp: ts2,
                level: LogLevel::Warn,
                props: vec![Prop {
                    key: "deviceId".into(),
                    value: "dev_small".into(),
                }],
                msg: "msg2".into(),
                ..Default::default()
            },
        ];

        // Create an orphan segment containing those logs.
        let mut seg = LogSegment::new();
        for log in &logs {
            seg.add_log_entry(log.clone());
        }
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
                first_timestamp: ts1,
                last_timestamp: ts2,
                original_size: orig,
                compressed_size: comp_size,
                logs_count: logs.len() as u64,
            })
            .await
            .unwrap();
        std::fs::write(ctx.logs_path().join(format!("{}.log", seg_id)), comp).unwrap();

        // Run the merger – it should process the orphan and flush immediately.
        let mut merger = DeviceMerger::new(ctx.clone());
        assert!(merger.run_once().await.unwrap(), "run_once should report work done");

        // The original orphan segment must be gone.
        let orphan = ctx
            .db
            .find_segments_without_device(None)
            .await
            .unwrap();
        assert!(orphan.is_empty(), "orphan segment should have been removed");

        // A new device-specific segment must exist and contain all logs.
        let segs = ctx
            .db
            .find_segments(&crate::types::GetSegmentsQuery::default())
            .await
            .unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].device_id.as_deref(), Some("dev_small"));
        assert_eq!(segs[0].logs_count, logs.len() as u64);
    }

	/// A very large orphan segment (more than `TARGET_SEGMENT_SIZE` logs for a
    /// single device) should be split into *multiple* device‑specific segments
    /// without losing any entries.
    #[tokio::test]
    async fn large_buffer_split_no_loss() {
        use super::TARGET_SEGMENT_SIZE;

        let (ctx, _dir) = prepare_ctx().await;
        let base_ts = chrono::Utc::now();
        let total_logs = TARGET_SEGMENT_SIZE + 1; // guarantees at least two flushes
        let mut raw_logs: Vec<LogEntry> = Vec::with_capacity(total_logs);
        for i in 0..total_logs {
            raw_logs.push(LogEntry {
                timestamp: base_ts + chrono::Duration::seconds(i as i64),
                level: LogLevel::Debug,
                props: vec![Prop {
                    key: "deviceId".into(),
                    value: "dev_big".into(),
                }],
                msg: format!("log {}", i),
                ..Default::default()
            });
        }

        // Wrap into a single orphan segment.
        let mut seg = LogSegment::new();
        for log in &raw_logs {
            seg.add_log_entry(log.clone());
        }
        seg.sort();
        let mut buf = Vec::new();
        seg.serialize(&mut buf);
        let orig_size = buf.len();
        let comp = zstd::encode_all(std::io::Cursor::new(buf), 0).unwrap();
        let comp_size = comp.len();
        let seg_id = ctx
            .db
            .new_segment(NewSegmentArgs {
                device_id: None,
                first_timestamp: raw_logs.first().unwrap().timestamp,
                last_timestamp: raw_logs.last().unwrap().timestamp,
                original_size: orig_size,
                compressed_size: comp_size,
                logs_count: total_logs as u64,
            })
            .await
            .unwrap();
        std::fs::write(ctx.logs_path().join(format!("{}.log", seg_id)), comp).unwrap();

        // Run the merger.
        let mut merger = DeviceMerger::new(ctx.clone());
        assert!(merger.run_once().await.unwrap(), "merger should process the big segment");

        // No orphan segments should remain.
        let orphan = ctx.db.find_segments_without_device(None).await.unwrap();
        assert!(orphan.is_empty(), "orphan should be gone");

        // All logs must now live in device‑specific segments, possibly >1.
        let segs = ctx
            .db
            .find_segments(&crate::types::GetSegmentsQuery::default())
            .await
            .unwrap();
        let mut total_persisted = 0;
        for s in &segs {
            assert_eq!(s.device_id.as_deref(), Some("dev_big"));
            total_persisted += s.logs_count;
        }
        assert_eq!(total_persisted, total_logs as u64, "every log must be preserved");
    }

    /// An orphan segment containing logs from *multiple* devices should end up
    /// creating exactly one segment per device—with each segment containing the
    /// correct subset of logs.
    #[tokio::test]
    async fn multiple_devices_no_loss() {
        let (ctx, _dir) = prepare_ctx().await;
        let base_ts = chrono::Utc::now();

        let dev_a_logs: Vec<LogEntry> = (0..3)
            .map(|i| LogEntry {
                timestamp: base_ts + chrono::Duration::seconds(i),
                level: LogLevel::Info,
                props: vec![Prop {
                    key: "deviceId".into(),
                    value: "devA".into(),
                }],
                msg: format!("A{}", i),
                ..Default::default()
            })
            .collect();

        let dev_b_logs: Vec<LogEntry> = (0..4)
            .map(|i| LogEntry {
                timestamp: base_ts + chrono::Duration::seconds(10 + i),
                level: LogLevel::Warn,
                props: vec![Prop {
                    key: "deviceId".into(),
                    value: "devB".into(),
                }],
                msg: format!("B{}", i),
                ..Default::default()
            })
            .collect();

        let all_logs: Vec<LogEntry> = dev_a_logs
            .iter()
            .chain(dev_b_logs.iter())
            .cloned()
            .collect();

        // One mixed orphan segment.
        let mut seg = LogSegment::new();
        for log in &all_logs {
            seg.add_log_entry(log.clone());
        }
        seg.sort();
        let mut buf = Vec::new();
        seg.serialize(&mut buf);
        let orig_size = buf.len();
        let comp = zstd::encode_all(std::io::Cursor::new(buf), 0).unwrap();
        let comp_size = comp.len();
        let seg_id = ctx
            .db
            .new_segment(NewSegmentArgs {
                device_id: None,
                first_timestamp: all_logs.first().unwrap().timestamp,
                last_timestamp: all_logs.last().unwrap().timestamp,
                original_size: orig_size,
                compressed_size: comp_size,
                logs_count: all_logs.len() as u64,
            })
            .await
            .unwrap();
        std::fs::write(ctx.logs_path().join(format!("{}.log", seg_id)), comp).unwrap();

        // Run the merger.
        let mut merger = DeviceMerger::new(ctx.clone());
        assert!(merger.run_once().await.unwrap());

        // Orphans gone.
        assert!(ctx
            .db
            .find_segments_without_device(None)
            .await
            .unwrap()
            .is_empty());

        // Should have exactly two segments: devA and devB, with correct counts.
        let segs = ctx
            .db
            .find_segments(&crate::types::GetSegmentsQuery::default())
            .await
            .unwrap();
        assert_eq!(segs.len(), 2, "one segment per device expected");

        for s in &segs {
            match s.device_id.as_deref() {
                Some("devA") => assert_eq!(s.logs_count, dev_a_logs.len() as u64),
                Some("devB") => assert_eq!(s.logs_count, dev_b_logs.len() as u64),
                other => panic!("unexpected device_id {:?}", other),
            }
        }
    }
}
