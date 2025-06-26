use chrono::DateTime;
use chrono::Utc;
use puppylog::LogLevel;
use puppylog::Prop;
use rusqlite::Connection;
use rusqlite::ToSql;
use serde::Deserialize;
use serde::Serialize;
use std::fs::create_dir_all;
use tokio::sync::Mutex;

use crate::config::db_path;
use crate::segment::SegmentMeta;
use crate::types::GetSegmentsQuery;
use crate::types::SortDir;
use std::collections::HashMap;

struct Migration {
	id: u32,
	name: &'static str,
	sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
	Migration {
		id: 20250212,
		name: "init_database",
		sql: r#"
            CREATE TABLE devices (
                id TEXT PRIMARY KEY,
                send_logs BOOLEAN NOT NULL DEFAULT false,
                filter_level INT NOT NULL DEFAULT 3,
                logs_size INTEGER NOT NULL DEFAULT 0,
                logs_count INTEGER NOT NULL DEFAULT 0,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                last_upload_at TIMESTAMP
            );
            CREATE TABLE log_segments (
                id INTEGER PRIMARY KEY,
                bucket_id INTEGER,
                first_timestamp TIMESTAMP NOT NULL,
                last_timestamp TIMESTAMP NOT NULL,
                original_size INTEGER NOT NULL,
                compressed_size INTEGER,
                logs_count INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
        "#,
	},
	Migration {
		id: 20250226,
		name: "add_send_interval_and_metadata",
		sql: r#"
            ALTER TABLE devices ADD COLUMN send_interval INTEGER NOT NULL DEFAULT 60;
            CREATE TABLE IF NOT EXISTS device_props (
                device_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY (device_id, key, value),
                FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
            );
        "#,
	},
	Migration {
		id: 20250321,
		name: "segment_props",
		sql: r#"
			CREATE TABLE segment_props (
					segment_id INTEGER NOT NULL,
					key TEXT NOT NULL,
					value TEXT NOT NULL,
					PRIMARY KEY (segment_id, key, value),
					FOREIGN KEY (segment_id) REFERENCES log_segments(id)
			);
		"#,
	},
	Migration {
		id: 20250614,
		name: "segment_device_id",
		sql: r#"
			ALTER TABLE log_segments ADD COLUMN device_id TEXT;
			CREATE INDEX IF NOT EXISTS log_segments_device_id_idx ON log_segments(device_id);
		"#,
	},
];

pub fn open_db() -> Connection {
	if cfg!(test) {
		println!("Opening in-memory database for tests");
		return Connection::open_in_memory().unwrap();
	}
	let path = db_path();
	if !path.exists() {
		create_dir_all(path.parent().unwrap()).unwrap();
	}
	Connection::open(db_path()).unwrap()
}

pub struct NewSegmentArgs {
	pub device_id: Option<String>,
	pub first_timestamp: chrono::DateTime<chrono::Utc>,
	pub last_timestamp: chrono::DateTime<chrono::Utc>,
	pub original_size: usize,
	pub compressed_size: usize,
	pub logs_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
	pub id: String,
	pub send_logs: bool,
	pub filter_level: LogLevel,
	pub send_interval: u32,
	pub logs_size: usize,
	pub logs_count: usize,
	pub created_at: DateTime<Utc>,
	pub last_upload_at: Option<DateTime<Utc>>,
	pub props: Vec<MetaProp>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct MetaProp {
	pub key: String,
	pub value: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct UpdateDevicesSettings {
	pub filter_props: Vec<MetaProp>,
	pub send_logs: bool,
	pub send_interval: u32,
	pub level: LogLevel,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDeviceSettings {
	pub send_logs: bool,
	pub filter_level: LogLevel,
	pub send_interval: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentsMetadata {
	pub segment_count: u32,
	pub original_size: u64,
	pub compressed_size: u64,
	pub logs_count: u64,
}

fn load_device_metadata_locked(
	conn: &Connection,
	device_id: &str,
) -> anyhow::Result<Vec<MetaProp>> {
	let mut stmt =
		conn.prepare(r#"SELECT key, value FROM device_props WHERE device_id = ?1 ORDER BY key"#)?;
	let mut rows = stmt.query([device_id])?;
	let mut props = Vec::new();
	while let Some(row) = rows.next()? {
		props.push(MetaProp {
			key: row.get(0)?,
			value: row.get(1)?,
		});
	}
	Ok(props)
}

#[derive(Debug)]
pub struct DB {
	conn: Mutex<Connection>,
}

impl DB {
	pub fn new(mut conn: Connection) -> Self {
		run_migrations(&mut conn).unwrap();
		DB {
			conn: Mutex::new(conn),
		}
	}

	pub async fn update_device_stats(
		&self,
		device_id: &str,
		logs_size: usize,
		logs_count: usize,
	) -> anyhow::Result<()> {
		let conn = &mut self.conn.lock().await;
		let tx = conn.transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO devices (id, logs_size, logs_count, last_upload_at)
				 VALUES (?1, ?2, ?3, current_timestamp)
				 ON CONFLICT(id) DO UPDATE SET
					 logs_size = devices.logs_size + ?2,
					 logs_count = devices.logs_count + ?3,
					 last_upload_at = current_timestamp",
			)?;
			stmt.execute([
				&device_id as &dyn ToSql,
				&logs_size as &dyn ToSql,
				&logs_count as &dyn ToSql,
			])?;
		}
		tx.commit()?;
		Ok(())
	}

	pub async fn get_devices(&self) -> anyhow::Result<Vec<Device>> {
		let conn = self.conn.lock().await;
		let mut stmt = conn.prepare(
			r#"
            SELECT
                id,
                send_logs,
                filter_level,
                logs_size,
                logs_count,
                created_at,
                last_upload_at,
                send_interval
            FROM devices
            "#,
		)?;
		let mut rows = stmt.query([])?;
		let mut list = Vec::new();
		while let Some(row) = rows.next()? {
			let device_id: String = row.get(0)?;
			let props = load_device_metadata_locked(&conn, &device_id)?;
			list.push(Device {
				id: device_id,
				send_logs: row.get(1)?,
				filter_level: LogLevel::from_i64(row.get(2)?),
				logs_size: row.get(3)?,
				logs_count: row.get(4)?,
				created_at: row.get(5)?,
				last_upload_at: row.get(6)?,
				send_interval: row.get(7)?,
				props,
			});
		}
		Ok(list)
	}

	pub async fn get_or_create_device(&self, device_id: &str) -> anyhow::Result<Device> {
		let conn = self.conn.lock().await;
		let now = chrono::Utc::now();

		let mut stmt = conn.prepare(
			"INSERT INTO devices 
			  (id, send_logs, filter_level, logs_size, logs_count, created_at, last_upload_at, send_interval)
			 VALUES 
			  (?, ?, ?, ?, ?, ?, ?, ?)
			 ON CONFLICT(id) DO UPDATE SET 
			  id = id
			 RETURNING id, send_logs, filter_level, send_interval, logs_size, logs_count, created_at, last_upload_at"
		)?;

		let default_send_logs = false;
		let default_filter_level = LogLevel::Info.to_u8();
		let default_send_interval = 500;
		let default_logs_size = 0;
		let default_logs_count = 0;

		let mut rows = stmt.query(rusqlite::params![
			device_id,
			default_send_logs,
			default_filter_level,
			default_logs_size,
			default_logs_count,
			now,
			now,
			default_send_interval,
		])?;

		if let Some(row) = rows.next()? {
			Ok(Device {
				id: row.get(0)?,
				send_logs: row.get(1)?,
				filter_level: LogLevel::from_i64(row.get(2)?),
				send_interval: row.get(3)?,
				logs_size: row.get(4)?,
				logs_count: row.get(5)?,
				created_at: row.get(6)?,
				last_upload_at: row.get(7)?,
				props: Vec::new(),
			})
		} else {
			Err(anyhow::anyhow!("Failed to get or create device"))
		}
	}

	pub async fn update_device_settings(&self, device_id: &str, payload: &UpdateDeviceSettings) {
		let conn = &mut self.conn.lock().await;
		let mut stmt = conn
			.prepare(
				"INSERT INTO devices (id, send_logs, filter_level, send_interval)
			VALUES (?1, ?2, ?3, ?4)
			ON CONFLICT(id) DO UPDATE SET
				send_logs = ?2,
				filter_level = ?3,
				send_interval = ?4",
			)
			.unwrap();
		stmt.execute([
			&device_id as &dyn ToSql,
			&payload.send_logs as &dyn ToSql,
			&payload.filter_level.to_u8() as &dyn ToSql,
			&payload.send_interval as &dyn ToSql,
		])
		.unwrap();
	}

	pub async fn update_device_metadata(
		&self,
		device_id: &str,
		metadata: &[MetaProp],
	) -> anyhow::Result<()> {
		let conn = self.conn.lock().await;
		let tx = conn.unchecked_transaction()?;
		{
			tx.execute(
				"INSERT OR IGNORE INTO devices (id) VALUES (?1)",
				[device_id],
			)?;
			tx.execute("DELETE FROM device_props WHERE device_id = ?1", [device_id])?;
			let mut ins_stmt = tx.prepare(
				r#"
				INSERT INTO device_props (device_id, key, value)
				VALUES (?1, ?2, ?3)
				"#,
			)?;
			for prop in metadata {
				ins_stmt.execute(rusqlite::params![device_id, prop.key, prop.value])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	// ------------------------------------------------------------------------
	// Bulk Update Example in a Single Query
	// ------------------------------------------------------------------------

	/// Update multiple devices at once (send_logs, filter_level, send_interval),
	/// but **only** those that match **all** of the given key-value `filter_props`.
	///
	/// Implements it as **one single UPDATE statement** using `EXISTS` subqueries.
	///
	/// **Explanation**: For each `MetaProp`, we append an `AND EXISTS(...)` condition.
	/// This ensures the device has that (key,value) pair in `device_props`.
	/// If `filter_props` is empty, we simply update all devices (no filter).
	pub async fn update_devices_settings(
		&self,
		payload: &UpdateDevicesSettings,
	) -> anyhow::Result<()> {
		let conn = self.conn.lock().await;
		let mut query = r#"
            UPDATE devices
               SET send_logs    = ?1,
                   send_interval= ?2,
                   filter_level = ?3
               WHERE 1
        "#
		.to_string();

		let mut params: Vec<Box<dyn ToSql>> = Vec::new();
		params.push(Box::new(payload.send_logs));
		params.push(Box::new(payload.send_interval as i32));
		params.push(Box::new(payload.level.to_u8()));

		for prop in &payload.filter_props {
			query.push_str(
				r#"
                  AND EXISTS (
                    SELECT 1 FROM device_props dp
                    WHERE dp.device_id = devices.id
                      AND dp.key = ?
                      AND dp.value = ?
                  )
                "#,
			);
			params.push(Box::new(prop.key.clone()));
			params.push(Box::new(prop.value.clone()));
		}

		let mut stmt = conn.prepare(&query)?;
		stmt.execute(rusqlite::params_from_iter(params.iter().map(|p| &**p)))?;
		Ok(())
	}

	pub async fn new_segment(&self, args: NewSegmentArgs) -> anyhow::Result<u32> {
		let mut conn = self.conn.lock().await;
		let tx = conn.transaction()?;
		let new_id = {
			let mut stmt = tx.prepare(
                               "INSERT INTO log_segments (device_id, first_timestamp, last_timestamp, original_size, compressed_size, logs_count)
                                VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
                        )?;
			stmt.execute([
				&args.device_id as &dyn ToSql,
				&args.first_timestamp as &dyn ToSql,
				&args.last_timestamp as &dyn ToSql,
				&args.original_size as &dyn ToSql,
				&args.compressed_size as &dyn ToSql,
				&args.logs_count as &dyn ToSql,
			])?;
			tx.last_insert_rowid()
		};
		tx.commit()?;
		Ok(new_id as u32)
	}

	pub async fn find_segments(
		&self,
		query: &GetSegmentsQuery,
	) -> anyhow::Result<Vec<SegmentMeta>> {
		let conn = self.conn.lock().await;
		let mut sql = String::from(
			"SELECT id, device_id, first_timestamp, last_timestamp, \
             original_size, compressed_size, logs_count, created_at \
             FROM log_segments",
		);
		let mut conditions: Vec<String> = Vec::new();
		let mut params: Vec<&dyn ToSql> = Vec::new();
		let mut limit_param: i64 = 0;

		if query.start.is_some() || query.end.is_some() {
			let mut condition = String::new();
			if let Some(start) = &query.start {
				condition.push_str("last_timestamp > ?");
				params.push(start);
			}
			if let Some(end) = &query.end {
				if !condition.is_empty() {
					condition.push_str(" AND ");
				}
				condition.push_str("first_timestamp <= ?");
				params.push(end);
			}
			conditions.push(condition);
		}

		if let Some(ids) = &query.device_ids {
			log::info!("device_ids: {:?}", ids);
			if !ids.is_empty() {
				let placeholders = std::iter::repeat("?")
					.take(ids.len())
					.collect::<Vec<_>>()
					.join(", ");
				conditions.push(format!("device_id IN ({})", placeholders));
				for id in ids {
					params.push(id as &dyn ToSql);
				}
			} else {
				conditions.push("1 = 0".to_string());
			}
		}

		if !conditions.is_empty() {
			sql.push_str(" WHERE ");
			sql.push_str(&conditions.join(" AND "));
		}

		let dir = match query.sort {
			Some(SortDir::Asc) => "ASC",
			Some(SortDir::Desc) => "DESC",
			None => "DESC",
		};
		sql.push_str(" ORDER BY last_timestamp ");
		sql.push_str(dir);
		if let Some(cnt) = query.count {
			sql.push_str(" LIMIT ?");
			limit_param = cnt as i64;
			params.push(&limit_param);
		}

		let mut stmt = conn.prepare(&sql)?;
		let mut rows = stmt.query(params.as_slice())?;
		let mut metas = Vec::new();
		while let Some(row) = rows.next()? {
			metas.push(SegmentMeta {
				id: row.get(0)?,
				device_id: row.get(1)?,
				first_timestamp: row.get(2)?,
				last_timestamp: row.get(3)?,
				original_size: row.get(4)?,
				compressed_size: row.get(5)?,
				logs_count: row.get(6)?,
				created_at: row.get(7)?,
			});
		}

		Ok(metas)
	}

	/// Return the `last_timestamp` of the newest segment.
	///
	/// * `before = Some(ts)` → newest segment whose `last_timestamp` < `ts`
	/// * `before = None`     → absolute newest segment (no upper bound)
	///
	/// Returns `Ok(None)` if no segment matches.
	pub async fn prev_segment_end(
		&self,
		before: Option<chrono::DateTime<chrono::Utc>>,
		device_ids: Option<&[String]>,
	) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
		use rusqlite::OptionalExtension;

		let conn = self.conn.lock().await;

		let mut sql = String::from("SELECT last_timestamp FROM log_segments");
		let mut conditions: Vec<String> = Vec::new();
		let mut params: Vec<&dyn ToSql> = Vec::new();

		if let Some(ref b) = before {
			conditions.push("last_timestamp < ?".into());
			params.push(b as &dyn ToSql);
		}

		if let Some(ids) = device_ids {
			if ids.is_empty() {
				return Ok(None);
			}
			let placeholders = std::iter::repeat("?")
				.take(ids.len())
				.collect::<Vec<_>>()
				.join(", ");
			conditions.push(format!("device_id IN ({})", placeholders));
			for id in ids {
				params.push(id as &dyn ToSql);
			}
		}

		if !conditions.is_empty() {
			sql.push_str(" WHERE ");
			sql.push_str(&conditions.join(" AND "));
		}

		sql.push_str(" ORDER BY last_timestamp DESC LIMIT 1");

		log::info!("prev_segment_end SQL: {}", sql);

		let mut stmt = conn.prepare(&sql)?;
		let ts: Option<chrono::DateTime<chrono::Utc>> = stmt
			.query_row(params.as_slice(), |row| row.get(0))
			.optional()?;

		Ok(ts)
	}

	pub async fn fetch_segment(&self, segment: u32) -> anyhow::Result<SegmentMeta> {
		let conn = self.conn.lock().await;
		let segment = conn.query_row("select device_id, first_timestamp, last_timestamp, original_size, compressed_size, logs_count, created_at from log_segments where id = ?1", [segment], |row| {
                       let device_id: Option<String> = row.get(0)?;
                       let first_timestamp: DateTime<Utc> = row.get(1)?;
                       let last_timestamp: DateTime<Utc> = row.get(2)?;
                       let original_size: usize = row.get(3)?;
                       let compressed_size: usize = row.get(4)?;
                       let logs_count: u64 = row.get(5)?;
                       let created_at: DateTime<Utc> = row.get(6)?;
                       Ok(SegmentMeta {
                               id: segment,
                               device_id,
                               first_timestamp,
                               last_timestamp,
                               original_size,
                               compressed_size,
                               logs_count,
                               created_at,
                       })
		})?;
		Ok(segment)
	}

	pub async fn find_segments_without_device(
		&self,
		limit: Option<u32>,
	) -> anyhow::Result<Vec<SegmentMeta>> {
		let conn = self.conn.lock().await;
		let mut sql = String::from(
			"SELECT id, device_id, first_timestamp, last_timestamp, original_size, compressed_size, logs_count, created_at FROM log_segments WHERE device_id IS NULL ORDER BY id",
		);
		let mut limit_param: i64 = 0;
		let params: Vec<&dyn ToSql> = if let Some(lim) = limit {
			sql.push_str(" LIMIT ?");
			limit_param = lim as i64;
			vec![&limit_param]
		} else {
			Vec::new()
		};

		let mut stmt = conn.prepare(&sql)?;
		let mut rows = stmt.query(params.as_slice())?;
		let mut metas = Vec::new();
		while let Some(row) = rows.next()? {
			metas.push(SegmentMeta {
				id: row.get(0)?,
				device_id: row.get(1)?,
				first_timestamp: row.get(2)?,
				last_timestamp: row.get(3)?,
				original_size: row.get(4)?,
				compressed_size: row.get(5)?,
				logs_count: row.get(6)?,
				created_at: row.get(7)?,
			});
		}

		Ok(metas)
	}

	pub async fn delete_segment(&self, segment: u32) -> anyhow::Result<()> {
		let mut conn = self.conn.lock().await;
		let tx = conn.transaction()?;
		tx.execute("DELETE FROM segment_props WHERE segment_id = ?1", [segment])?;
		tx.execute("DELETE FROM log_segments WHERE id = ?1", [segment])?;
		tx.commit()?;
		Ok(())
	}

	pub async fn fetch_segments_metadata(&self) -> anyhow::Result<SegmentsMetadata> {
		let conn = self.conn.lock().await;
		let mut stmt = conn.prepare("SELECT COUNT(*), COALESCE(SUM(original_size), 0), COALESCE(SUM(compressed_size), 0), COALESCE(SUM(logs_count), 0) FROM log_segments")?;
		let mut rows = stmt.query([])?;
		let row = rows.next()?.unwrap();
		Ok(SegmentsMetadata {
			segment_count: row.get(0)?,
			original_size: row.get(1)?,
			compressed_size: row.get(2)?,
			logs_count: row.get(3)?,
		})
	}

	pub async fn upsert_segment_props(
		&self,
		segment: u32,
		props: impl Iterator<Item = &Prop>,
	) -> anyhow::Result<()> {
		let mut conn = self.conn.lock().await;
		let tx = conn.transaction()?;
		{
			let mut ins_stmt = tx.prepare(
				"INSERT OR IGNORE INTO segment_props (segment_id, key, value) VALUES (?1, ?2, ?3)",
			)?;
			for prop in props {
				ins_stmt.execute(rusqlite::params![segment, prop.key, prop.value])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	pub async fn fetch_segment_props(&self, segment: u32) -> anyhow::Result<Vec<Prop>> {
		let conn = self.conn.lock().await;
		let mut stmt =
			conn.prepare("SELECT key, value FROM segment_props WHERE segment_id = ?1")?;
		let mut rows = stmt.query([segment])?;
		let mut props = Vec::new();
		while let Some(row) = rows.next()? {
			props.push(Prop {
				key: row.get(0)?,
				value: row.get(1)?,
			});
		}
		Ok(props)
	}

	pub async fn fetch_segments_props(
		&self,
		segment_ids: &[u32],
	) -> anyhow::Result<HashMap<u32, Vec<Prop>>> {
		// SQLite can bind at most 999 parameters per statement.  We batch
		// the request in chunks of that size so very large queries stay safe
		// and perform well.
		const MAX_SQL_PARAMS: usize = 999;

		let conn = self.conn.lock().await;

		if segment_ids.is_empty() {
			return Ok(HashMap::new());
		}

		let mut map: HashMap<u32, Vec<Prop>> = HashMap::new();

		for chunk in segment_ids.chunks(MAX_SQL_PARAMS) {
			// Build a "?, ?, …" placeholder list sized to this chunk.
			let placeholders = std::iter::repeat("?")
				.take(chunk.len())
				.collect::<Vec<_>>()
				.join(", ");

			let sql = format!(
				"SELECT segment_id, key, value \
				 FROM segment_props \
				 WHERE segment_id IN ({})",
				placeholders
			);

			let mut stmt = conn.prepare(&sql)?;
			let params: Vec<&dyn ToSql> = chunk.iter().map(|id| id as &dyn ToSql).collect();
			let mut rows = stmt.query(rusqlite::params_from_iter(params))?;

			while let Some(row) = rows.next()? {
				let sid: u32 = row.get(0)?;
				let prop = Prop {
					key: row.get(1)?,
					value: row.get(2)?,
				};
				map.entry(sid).or_default().push(prop);
			}
		}

		Ok(map)
	}
}

pub fn run_migrations(conn: &mut Connection) -> anyhow::Result<()> {
	log::info!("running migrations");
	conn.execute(
		"CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
		(),
	)?;
	let applied_migrations: Vec<u32> = {
		let mut stmt = conn.prepare("SELECT id FROM migrations")?;
		let m = stmt.query_map((), |row| row.get(0))?;
		m.filter_map(Result::ok).collect()
	};
	let mut pending_migrations: Vec<&Migration> = MIGRATIONS
		.iter()
		.filter(|migration| !applied_migrations.contains(&migration.id))
		.collect();
	pending_migrations.sort_by_key(|migration| migration.id);
	if !pending_migrations.is_empty() {
		for migration in &pending_migrations {
			log::info!("applying migration {}: {}", migration.id, migration.name);
			let tx = conn.transaction()?;
			tx.execute_batch(migration.sql)?;
			tx.execute(
				"INSERT INTO migrations (id, name) VALUES (?1, ?2)",
				[&migration.id as &dyn ToSql, &migration.name as &dyn ToSql],
			)?;
			tx.commit()?;
			log::info!("migration {} applied successfully.", migration.id);
		}
	} else {
		log::info!("No new migrations to apply.");
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rusqlite::Connection;

	#[tokio::test]
	async fn delete_segment_removes_props() {
		let conn = Connection::open_in_memory().unwrap();
		let db = DB::new(conn);

		let now = Utc::now();
		let segment = db
			.new_segment(NewSegmentArgs {
				device_id: None,
				first_timestamp: now,
				last_timestamp: now,
				original_size: 1,
				compressed_size: 1,
				logs_count: 1,
			})
			.await
			.unwrap();

		let prop = Prop {
			key: "kind".to_string(),
			value: "value".to_string(),
		};
		db.upsert_segment_props(segment, [prop.clone()].iter())
			.await
			.unwrap();
		assert_eq!(db.fetch_segment_props(segment).await.unwrap().len(), 1);

		db.delete_segment(segment).await.unwrap();

		assert!(db.fetch_segment(segment).await.is_err());
		assert!(db.fetch_segment_props(segment).await.unwrap().is_empty());
	}

	#[tokio::test]
	async fn find_segments_overlap_start_inside_segment() {
		use crate::segment::SegmentMeta; // for clarity

		let conn = Connection::open_in_memory().unwrap();
		let db = DB::new(conn);

		// Create one segment: [first=now-2h, last=now-1h]
		let now = Utc::now();
		let first_ts = now - chrono::Duration::hours(2);
		let last_ts = now - chrono::Duration::hours(1);
		let seg_id = db
			.new_segment(NewSegmentArgs {
				device_id: None,
				first_timestamp: first_ts,
				last_timestamp: last_ts,
				original_size: 100,
				compressed_size: 50,
				logs_count: 10,
			})
			.await
			.unwrap();

		// Query window: start = now-90 min (inside segment), end = now
		// Expect the segment to be returned because the interval overlaps.
		let metas = db
			.find_segments(&GetSegmentsQuery {
				start: Some(now - chrono::Duration::minutes(90)),
				end: Some(now),
				device_ids: None,
				count: None,
				sort: None,
			})
			.await
			.unwrap();

		assert_eq!(
			metas.iter().map(|m| m.id).collect::<Vec<_>>(),
			vec![seg_id],
			"segment should be returned when query start is inside its time span"
		);
	}

	#[tokio::test]
	async fn find_segments_without_device() {
		let conn = Connection::open_in_memory().unwrap();
		let db = DB::new(conn);

		let now = Utc::now();

		let no_dev = db
			.new_segment(NewSegmentArgs {
				device_id: None,
				first_timestamp: now,
				last_timestamp: now,
				original_size: 1,
				compressed_size: 1,
				logs_count: 1,
			})
			.await
			.unwrap();

		let _with_dev = db
			.new_segment(NewSegmentArgs {
				device_id: Some("dev1".into()),
				first_timestamp: now,
				last_timestamp: now,
				original_size: 1,
				compressed_size: 1,
				logs_count: 1,
			})
			.await
			.unwrap();

		let metas = db.find_segments_without_device(None).await.unwrap();
		assert_eq!(metas.len(), 1);
		assert_eq!(metas[0].id, no_dev);
		assert!(metas[0].device_id.is_none());
	}

	#[tokio::test]
	async fn prev_segment_end_filters_device() {
		let conn = Connection::open_in_memory().unwrap();
		let db = DB::new(conn);

		let now = Utc::now();
		let ts_dev1 = now - chrono::Duration::hours(30);
		let ts_dev2 = now - chrono::Duration::hours(5);

		db.new_segment(NewSegmentArgs {
			device_id: Some("dev1".into()),
			first_timestamp: ts_dev1,
			last_timestamp: ts_dev1,
			original_size: 1,
			compressed_size: 1,
			logs_count: 1,
		})
		.await
		.unwrap();

		db.new_segment(NewSegmentArgs {
			device_id: Some("dev2".into()),
			first_timestamp: ts_dev2,
			last_timestamp: ts_dev2,
			original_size: 1,
			compressed_size: 1,
			logs_count: 1,
		})
		.await
		.unwrap();

		let found = db
			.prev_segment_end(Some(now), Some(&["dev1".to_string()]))
			.await
			.unwrap()
			.unwrap();

		assert_eq!(found, ts_dev1);
	}
}
