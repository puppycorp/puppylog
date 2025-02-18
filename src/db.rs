use std::fs::create_dir_all;

use anyhow::bail;
use chrono::DateTime;
use chrono::Utc;
use puppylog::LogEntry;
use puppylog::LogLevel;
use puppylog::QueryAst;
use rusqlite::Connection;
use rusqlite::ToSql;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::config::db_path;
use crate::segment::SegmentMeta;
use crate::UpdateDeviceSettings;

struct Migration {
    id: u32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
	Migration {
		id: 20250212,
		name: "init_database",
		sql: r"
		create table devices (
			id text primary key,
			send_logs boolean not null default false,
			filter_level int not null default 3,
			logs_size integer not null default 0,
			logs_count integer not null default 0,
			created_at timestamp default current_timestamp,
			last_upload_at timestamp
		);
		create table logs (
			random integer,
			timestamp timestamp not null,
			level int not null,
			msg text not null,
			props bytes,
			primary key (random, timestamp)
		);
		create index logs_timestamp on logs (timestamp);
		create index logs_level on logs (level);
		create table log_segments (
			id integer primary key,
			bucket_id integer null,
			first_timestamp timestamp not null,
			last_timestamp timestamp not null,
			original_size integer not null,
			compressed_size integer,
			logs_count integer not null,
			created_at timestamp default current_timestamp
		);
		"
	}
];

pub fn open_db() -> Connection {
	let path = db_path();
	if !path.exists() {
		create_dir_all(path.parent().unwrap()).unwrap();
	}
	Connection::open(db_path()).unwrap()
}

pub struct NewSegmentArgs {
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
	pub logs_size: usize,
	pub logs_count: usize,
	pub created_at: DateTime<Utc>,
	pub last_upload_at: Option<DateTime<Utc>>,
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

	pub async fn update_device_metadata(&self, device_id: &str, logs_size: usize, logs_count: usize) -> anyhow::Result<()> {
		let conn = &mut self.conn.lock().await;
		let tx = conn.transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO devices (id, logs_size, logs_count, last_upload_at)
				 VALUES (?1, ?2, ?3, current_timestamp)
				 ON CONFLICT(id) DO UPDATE SET
					 logs_size = devices.logs_size + ?2,
					 logs_count = devices.logs_count + ?3,
					 last_upload_at = current_timestamp"
			)?;
			stmt.execute(&[&device_id as &dyn ToSql, &logs_size as &dyn ToSql, &logs_count as &dyn ToSql])?;
		}
		tx.commit()?;
		log::info!("saved device metadata: {} {} {}", device_id, logs_size, logs_count);
		Ok(())
	}

	pub async fn get_devices(&self) -> anyhow::Result<Vec<Device>> {
		let conn = self.conn.lock().await;
		let mut stmt = conn.prepare("SELECT id, send_logs, filter_level, logs_size, logs_count, created_at, last_upload_at FROM devices")?;
		let mut rows = stmt.query([])?;
		let mut devices = Vec::new();
		while let Some(row) = rows.next()? {
			devices.push(Device {
				id: row.get(0)?,
				send_logs: row.get(1)?,
				filter_level: LogLevel::from_i64(row.get(2)?),
				logs_size: row.get(3)?,
				logs_count: row.get(4)?,
				created_at: row.get(5)?,
				last_upload_at: row.get(6)?,
			});
		}
		Ok(devices)
	}

	pub async fn get_device(&self, device_id: &str) -> anyhow::Result<Option<Device>> {
		let conn = self.conn.lock().await;
		let mut stmt = conn.prepare("SELECT id, send_logs, filter_level, logs_size, logs_count, created_at, last_upload_at FROM devices WHERE id = ?")?;
		let mut rows = stmt.query([&device_id])?;
		if let Some(row) = rows.next()? {
			Ok(Some(Device {
				id: row.get(0)?,
				send_logs: row.get(1)?,
				filter_level: LogLevel::from_i64(row.get(2)?),
				logs_size: row.get(3)?,
				logs_count: row.get(4)?,
				created_at: row.get(5)?,
				last_upload_at: row.get(6)?,
			}))
		} else {
			Ok(None)
		}
	}

	pub async fn handle_device_upload(&self, device_id: &str, new_bytes: u32, logs: &[LogEntry]) -> anyhow::Result<()> {
		let conn = &mut self.conn.lock().await;
		let tx = conn.transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO devices (id, last_upload_at, bytes_sent)
				 VALUES (?1, CURRENT_TIMESTAMP, ?2)
				 ON CONFLICT(id) DO UPDATE SET
					 last_upload_at = CURRENT_TIMESTAMP,
					 bytes_sent = devices.bytes_sent + ?2"
			)?;
			stmt.execute(&[&device_id as &dyn ToSql, &new_bytes as &dyn ToSql])?;
			let mut stmt = tx.prepare("INSERT INTO logs (random, timestamp, level, msg, props) VALUES (?, ?, ?, ?, ?)")?;
			let mut props = Vec::with_capacity(4096);
			for log in logs {
				log.serialize_props(&mut props)?;
				stmt.execute(&[
					&log.random as &dyn ToSql,
					&log.timestamp as &dyn ToSql,
					&log.level.to_u8() as &dyn ToSql,
					&log.msg as &dyn ToSql,
					&props as &dyn ToSql,
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	pub async fn update_device_settings(&self, device_id: &str, payload: &UpdateDeviceSettings) {
		let conn = &mut self.conn.lock().await;
		let mut stmt = conn.prepare(
			"INSERT INTO devices (id, send_logs, filter_level)
			VALUES (?1, ?2, ?3)
			ON CONFLICT(id) DO UPDATE SET
				send_logs = ?2,
				filter_level = ?3"
		).unwrap();
		stmt.execute(&[
			&device_id as &dyn ToSql,
			&payload.send_logs as &dyn ToSql,
			&payload.filter_level.to_u8() as &dyn ToSql,
		]).unwrap();
	}

	pub async fn search_logs(&self, query: QueryAst) -> anyhow::Result<Vec<LogEntry>> {
		let conn = self.conn.lock().await;
		let end_date = query.end_date.unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::days(300));
		let limit = query.limit.unwrap_or(200);
		let mut stmt = conn.prepare("SELECT random, timestamp, level, msg, props FROM logs where timestamp < ? order by timestamp desc")?;
		let mut rows = stmt.query([end_date])?;
		let mut logs = Vec::new();
		while let Some(row) = rows.next()? {
			let mut log_entry = LogEntry {
				random: row.get(0)?,
				timestamp: row.get(1)?,
				level: LogLevel::from_i64(row.get(2)?),
				msg: row.get(3)?,
				props: Vec::with_capacity(1024),
				..Default::default()
			};
			let data = match row.get_ref(4)? {
				rusqlite::types::ValueRef::Blob(data) => data,
				_ => bail!("invalid data type for props"),
			};
			LogEntry::deserialize_props(&data, &mut log_entry.props);
			match query.matches(&log_entry) {
				Ok(true) => logs.push(log_entry),
				Ok(false) => continue,
				Err(e) => bail!(e),
			}
			if logs.len() >= limit {
				break;
			}
		}
		Ok(logs)
	}

	pub async fn new_segment(&self, args: NewSegmentArgs) -> anyhow::Result<u32> {
		let mut conn = self.conn.lock().await;
		let tx = conn.transaction()?;
		let new_id = {
			let mut stmt = tx.prepare(
				"INSERT INTO log_segments (first_timestamp, last_timestamp, original_size, compressed_size, logs_count)
				 VALUES (?1, ?2, ?3, ?4, ?5)"
			)?;
			stmt.execute(&[
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

	pub async fn get_segment_metadatas(&self) -> anyhow::Result<Vec<SegmentMeta>> {
		let conn = self.conn.lock().await;
		let mut stmt = conn.prepare("SELECT id, first_timestamp, last_timestamp, original_size, compressed_size, logs_count FROM log_segments")?;
		let mut rows = stmt.query([])?;
		let mut metas = Vec::new();
		while let Some(row) = rows.next()? {
			metas.push(SegmentMeta {
				id: row.get(0)?,
				first_timestamp: row.get(1)?,
				last_timestamp: row.get(2)?,
			});
		}
		Ok(metas)
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
    let mut pending_migrations: Vec<&Migration> = MIGRATIONS.iter()
        .filter(|migration| !applied_migrations.contains(&migration.id))
        .collect();
    pending_migrations.sort_by_key(|migration| migration.id);
    if !pending_migrations.is_empty() {
        for migration in &pending_migrations {
            log::info!("applying migration {}: {}", migration.id, migration.name);
            let tx = conn.transaction()?;
            tx.execute_batch(migration.sql)?;
            tx.execute("INSERT INTO migrations (id, name) VALUES (?1, ?2)", &[&migration.id as &dyn ToSql, &migration.name as &dyn ToSql])?;
            tx.commit()?;
            log::info!("migration {} applied successfully.", migration.id);
        }
    } else {
        log::info!("No new migrations to apply.");
    }
    Ok(())
}