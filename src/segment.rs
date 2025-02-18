use std::cmp::Ordering;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::num::NonZero;
use std::sync::Arc;
use bytes::Bytes;
use chrono::DateTime;
use chrono::Utc;
use lru::LruCache;
use puppylog::LogEntry;
use puppylog::LogEntryChunkParser;
use tokio::sync::Mutex;

use crate::config::log_path;

#[derive(Debug)]
pub struct LogIterator<'a> {
	pub buffer: &'a [LogEntry],
	pub offset: usize,
}

impl<'a> LogIterator<'a> {
	pub fn new(buffer: &'a [LogEntry], offset: usize) -> Self {
		LogIterator {
			buffer,
			offset
		}
	}
}

impl<'a> Iterator for LogIterator<'a> {
	type Item = &'a LogEntry;

	fn next(&mut self) -> Option<Self::Item> {
		if self.offset == 0 {
			return None;
		}
		let log = &self.buffer[self.offset - 1];
		self.offset -= 1;
		Some(log)
	}
}

pub const MAGIC: &str = "PUPPYLOGSEG";
pub const VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 13;

#[derive(Debug, Clone)]
pub struct SegmentMeta {
	pub id: u32,
	pub first_timestamp: DateTime<Utc>,
	pub last_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogSegment {
	pub buffer: Vec<LogEntry>
}

type ImmutableLogSegment = Arc<LogSegment>;

impl LogSegment {
	pub fn with_logs(mut logs: Vec<LogEntry>) -> Self {
		logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
		LogSegment {
			buffer: logs
		}
	}
	pub fn new() -> Self {
		LogSegment {
			buffer: Vec::new()
		}
	}
	pub fn iter(&self, end: DateTime<Utc>) -> LogIterator {
		let i = self.date_index(end);
		LogIterator::new(&self.buffer[..i], i)
	}
	fn date_index(&self, date: DateTime<Utc>) -> usize {
		self.buffer.binary_search_by(|log| {
			if log.timestamp > date {
				Ordering::Greater
			} else {
				Ordering::Less
			}
		}).unwrap_or_else(|idx| idx)
	}
	pub fn sort(&mut self) {
		self.buffer.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
	}

	pub fn add_log_entry(&mut self, log: LogEntry) {
		self.buffer.push(log);
	}

	pub fn serialize<W: Write>(&self, writer: &mut W) {
		writer.write_all(MAGIC.as_bytes()).unwrap();
		writer.write_all(&VERSION.to_be_bytes()).unwrap();
		for log in &self.buffer {
			log.serialize(writer);
		}
	}

	pub fn parse<R: Read>(reader: &mut R) -> Self {
		let mut header = [0u8; HEADER_SIZE];
		reader.read_exact(&mut header).unwrap();
		let magic = String::from_utf8_lossy(&header[0..11]);
		if magic != MAGIC {
			panic!("Invalid magic: {}", magic);
		}
		let version = u16::from_be_bytes(header[11..13].try_into().unwrap());
		if version != VERSION {
			panic!("Invalid version: {}", version);
		}
		let mut chunk_parser = LogEntryChunkParser::new();
		let mut chunk = [0u8; 4096];
		loop {
			let n = reader.read(&mut chunk).unwrap();
			if n == 0 {
				break;
			}
			chunk_parser.add_chunk(Bytes::copy_from_slice(&chunk[..n]));
		}

		LogSegment {
			buffer: chunk_parser.log_entries
		}
	}

	pub fn contains_date(&self, date: DateTime<Utc>) -> bool {
		if self.buffer.is_empty() {
			return false;
		}
		let first = self.buffer.first().unwrap();
		date >= first.timestamp
	}
}

pub struct LogSegmentsIterator<'a> {
    active: &'a LogSegment,
    old: &'a [Arc<LogSegment>],
	indexes: Vec<usize>,
}

impl<'a> LogSegmentsIterator<'a> {
    pub async fn new(segments: &'a LogSegmentManager, end: DateTime<Utc>) -> Self {
		let i = segments.current.date_index(end);
		let mut indexes = vec![0; segments.old.len()];
		indexes.push(i);
		for (i, segment) in segments.old.iter().enumerate() {
			indexes[i] = segment.date_index(end);
		}
        Self {
			active: &segments.current,
			old: &segments.old,
			indexes
        }
    }
}

impl<'a> Iterator for LogSegmentsIterator<'a> {
    type Item = LogEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // best_candidate will store (segment_index, candidate log entry)
        let mut best_candidate: Option<(usize, &LogEntry)> = None;

        // Iterate over all segments.
        // The first self.old.len() entries in self.indexes correspond to the old segments,
        // and the last entry corresponds to the active segment.
        for (i, &current_index) in self.indexes.iter().enumerate() {
            if current_index == 0 {
                // This segment is exhausted.
                continue;
            }
            // Fetch the candidate from the appropriate segment.
            let candidate = if i < self.old.len() {
                // For an old segment.
                &self.old[i].buffer[current_index - 1]
            } else {
                // For the active segment.
                &self.active.buffer[current_index - 1]
            };

            // If this candidate is more recent than the current best, choose it.
            best_candidate = match best_candidate {
                Some((_, best_log)) if candidate.timestamp > best_log.timestamp => Some((i, candidate)),
                None => Some((i, candidate)),
                _ => best_candidate,
            };
        }

        // If no candidate was found in any segment, we're done.
        if let Some((segment_idx, candidate)) = best_candidate {
            // Decrement the index for the chosen segment.
            self.indexes[segment_idx] -= 1;
            // Return a clone of the candidate log entry.
            Some(candidate.clone())
        } else {
            None
        }
    }
}

#[derive(Debug)]
struct LogSegmentArchiveInner {
	metas: Vec<SegmentMeta>,
	segments: LruCache<u32, ImmutableLogSegment>
}

#[derive(Debug)]
pub struct LogSegmentArchive {
	inner: Mutex<LogSegmentArchiveInner>
}

impl LogSegmentArchive {
	pub fn new(mut metas: Vec<SegmentMeta>) -> Self {
		metas.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
		for meta in &metas {
			log::info!("meta.timestamp: {}", meta.last_timestamp);
		}
		LogSegmentArchive {
			inner: Mutex::new(LogSegmentArchiveInner {
				metas,
				segments: LruCache::new(NonZero::new(10).unwrap())
			})
		}
	}

	pub async fn add_segment(&self, id: u32, segment: LogSegment) {
		let mut inner = self.inner.lock().await;
		inner.metas.push(SegmentMeta {
			id,
			first_timestamp: segment.buffer.first().unwrap().timestamp,
			last_timestamp: segment.buffer.last().unwrap().timestamp
		});
		inner.metas.sort_by(|a, b| b.first_timestamp.cmp(&a.first_timestamp));
		inner.segments.put(id, Arc::new(segment));
	}

	pub async fn get_relevant_segment(&self, date: DateTime<Utc>) -> Option<ImmutableLogSegment> {
		let mut inner = self.inner.lock().await;
		let mut id = None;
		for meta in &inner.metas {
			if date > meta.first_timestamp {
				log::info!("found relevant segment: {:?}", meta);
				id = Some(meta.id);
				break;
			}
		}
		match id {
			Some(id) => match inner.segments.get(&id) {
				Some(segment) => Some(segment.clone()),
				None => {
					let path = log_path().join(format!("{}.log", id));
					log::info!("loading segment from disk: {}", path.display());
					let file: File = File::open(path).unwrap();
					let mut decoder = zstd::Decoder::new(file).unwrap();
					let segment = LogSegment::parse(&mut decoder);
					let segment = Arc::new(segment);
					inner.segments.put(id, segment.clone());
					Some(segment)
				}
			},
			None => None
		}
	}
}

#[derive(Debug)]
pub struct LogSegmentManager {
	pub current: LogSegment,
	pub old: Vec<Arc<LogSegment>>
}

impl LogSegmentManager {
	pub fn new() -> Self {
		LogSegmentManager {
			current: LogSegment::new(),
			old: Vec::new(),
		}
	}

	pub fn with_logs(logs: Vec<LogEntry>) -> Self {
		LogSegmentManager {
			current: LogSegment::with_logs(logs),
			old: Vec::new(),
		}
	}

	pub fn rotate(&mut self) {
		let mut old = LogSegment::new();
		std::mem::swap(&mut old, &mut self.current);
		self.old.push(Arc::new(old));
	}

	pub async fn segment(&mut self) -> &mut LogSegment {
		&mut self.current
	}

	pub async fn iter<'a>(&'a self, end: DateTime<Utc>) -> LogSegmentsIterator<'a> {
		LogSegmentsIterator::new(self, end).await
	}

	pub fn for_each<F>(&self, end: DateTime<Utc>, mut callback: F)
    where
        F: FnMut(&LogEntry) -> bool,
    {
        let active_index = self.current.date_index(end);

        // Prepare indexes for all segments (old segments first, then active).
        let mut indexes: Vec<usize> = self
            .old
            .iter()
            .map(|seg| seg.date_index(end))
            .collect();
        indexes.push(active_index);

        // Loop until all segments are exhausted.
        loop {
            let mut best_candidate: Option<(usize, &LogEntry)> = None;

            // Iterate over all segments.
            // The first `self.old_segments.len()` entries in `indexes` refer to the old segments,
            // and the last entry corresponds to the active segment.
            for (i, &current_index) in indexes.iter().enumerate() {
                if current_index == 0 {
                    // This segment is exhausted.
                    continue;
                }
                let candidate = if i < self.old.len() {
                    // Reference from an old segment.
                    &self.old[i].buffer[current_index - 1]
                } else {
                    // Reference from the active segment.
                    &self.current.buffer[current_index - 1]
                };

                // Choose the candidate with the most recent timestamp.
                best_candidate = match best_candidate {
                    Some((_, best_log)) if candidate.timestamp > best_log.timestamp => {
                        Some((i, candidate))
                    }
                    None => Some((i, candidate)),
                    _ => best_candidate,
                };
            }

            // If no candidate is found, exit the loop.
            if let Some((segment_idx, candidate)) = best_candidate {
                // Decrement the index for the chosen segment.
                indexes[segment_idx] -= 1;
                // If the callback returns false, exit early.
                if !callback(candidate) {
                    break;
                }
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
	use std::io::Cursor;

use chrono::Duration;
use chrono::TimeZone;
use puppylog::LogLevel;
	use puppylog::Prop;
	use super::*;

	// #[test]
	// pub fn test_log_segment() {
	// 	let mut segment = LogSegment::new();
	// 	let log = LogEntry {
	// 		random: 0,
	// 		timestamp: Utc::now(),
	// 		level: LogLevel::Info,
	// 		msg: "Hello, world!".to_string(),
	// 		props: vec![Prop {
	// 			key: "key".to_string(),
	// 			value: "value".to_string(),
	// 		}],
	// 		..Default::default()
	// 	};
	// 	segment.add_log_entry(log.clone());

	// 	let mut iter = segment.iter(None);
	// 	let lof1 = iter.next().unwrap();
	// 	assert_eq!(log, lof1.clone());
	// 	assert!(iter.next().is_none());

	// 	let mut buff = Vec::new();
	// 	segment.serialize(&mut buff);
	// 	let mut reader = Cursor::new(buff);
	// 	let segment2 = LogSegment::parse(&mut reader);
	// 	assert_eq!(segment, segment2);
	// }

	// // A helper to create a dummy log entry.
    // fn dummy_log(timestamp: chrono::DateTime<Utc>, msg: &str) -> LogEntry {
    //     LogEntry {
    //         random: 0,
    //         timestamp,
    //         level: LogLevel::Info,
    //         msg: msg.to_string(),
    //         props: vec![Prop {
    //             key: "key".to_string(),
    //             value: "value".to_string(),
    //         }],
    //         ..Default::default()
    //     }
    // }
    
    // #[tokio::test]
    // async fn test_log_segments_iterator_returns_logs_in_descending_order_with_more_logs() {
    //     // Set up timestamps.
    //     let now = Utc::now();
    //     let timestamps = vec![
    //         now - Duration::seconds(1),
    //         now - Duration::seconds(2),
    //         now - Duration::seconds(3),
    //         now - Duration::seconds(4),
    //         now - Duration::seconds(5),
    //         now - Duration::seconds(6),
    //         now - Duration::seconds(7),
    //         now - Duration::seconds(8),
    //         now - Duration::seconds(9),
    //         now - Duration::seconds(10),
    //     ];

    //     // Active segment will contain three logs.
    //     let mut active_segment = LogSegment::new();
    //     active_segment.add_log_entry(dummy_log(timestamps[9], "active: oldest"));   // now - 10 secs
    //     active_segment.add_log_entry(dummy_log(timestamps[5], "active: middle"));     // now - 6 secs
    //     active_segment.add_log_entry(dummy_log(timestamps[0], "active: newest"));     // now - 1 sec

    //     // Old segment 1 with three logs.
    //     let mut old_segment1 = LogSegment::new();
    //     old_segment1.add_log_entry(dummy_log(timestamps[8], "old1: oldest"));         // now - 9 secs
    //     old_segment1.add_log_entry(dummy_log(timestamps[4], "old1: middle"));           // now - 5 secs
    //     old_segment1.add_log_entry(dummy_log(timestamps[2], "old1: newest"));           // now - 3 secs

    //     // Old segment 2 with three logs.
    //     let mut old_segment2 = LogSegment::new();
    //     old_segment2.add_log_entry(dummy_log(timestamps[7], "old2: oldest"));         // now - 8 secs
    //     old_segment2.add_log_entry(dummy_log(timestamps[6], "old2: middle"));           // now - 7 secs
    //     old_segment2.add_log_entry(dummy_log(timestamps[3], "old2: newest"));           // now - 4 secs

    //     // Create LogSegments with two old segments and one active segment.
    //     let segments = LogSegmentManager {
    //         current: active_segment,
    //         old: vec![old_segment1, old_segment2],
    //     };

    //     // We'll set the end time to now + 1 second to include all logs.
    //     let end_time = now + Duration::seconds(1);

    //     // Create the iterator.
    //     let mut iter = segments.iter(end_time).await;

    //     // Expected descending order:
    //     // 1. "active: newest"     (now - 1 sec)
    //     // 2. "old1: newest"       (now - 3 secs)
    //     // 3. "old2: newest"       (now - 4 secs)
    //     // 4. "old1: middle"       (now - 5 secs)
    //     // 5. "active: middle"     (now - 6 secs)
    //     // 6. "old2: middle"       (now - 7 secs)
    //     // 7. "old2: oldest"       (now - 8 secs)
    //     // 8. "old1: oldest"       (now - 9 secs)
    //     // 9. "active: oldest"     (now - 10 secs)
    //     let expected_order = vec![
    //         "active: newest",
    //         "old1: newest",
    //         "old2: newest",
    //         "old1: middle",
    //         "active: middle",
    //         "old2: middle",
    //         "old2: oldest",
    //         "old1: oldest",
    //         "active: oldest",
    //     ];

    //     for expected_msg in expected_order {
    //         let log = iter.next().expect("Expected log entry");
    //         assert_eq!(log.msg, expected_msg, "Expected '{}' but got '{}'", expected_msg, log.msg);
    //     }

    //     // Iterator should now be exhausted.
    //     assert!(iter.next().is_none());
    // }
}