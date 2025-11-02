use std::collections::HashMap;
use std::fs::create_dir_all;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::connection::SimpleConnection;
use diesel::dsl::{exists, now};
use diesel::expression::BoxableExpression;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, CustomizeConnection, Pool, PooledConnection};
use diesel::sql_types::{BigInt, Bool, Integer, Text};
use diesel::sqlite::{Sqlite, SqliteConnection};
use diesel::{insert_into, insert_or_ignore_into};
use puppylog::{LogLevel, Prop};
use serde::{Deserialize, Serialize};

use crate::config::db_path;
use crate::schema::{
	bucket_logs, device_props, devices, log_buckets, log_segments, migrations, segment_props,
};
use crate::segment::SegmentMeta;
use crate::types::{GetSegmentsQuery, SortDir};

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
	Migration {
		id: 20250624,
		name: "log_buckets",
		sql: r#"
                        CREATE TABLE log_buckets (
                                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                                        name TEXT NOT NULL UNIQUE,
                                        query TEXT NOT NULL DEFAULT '',
                                        created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                                        updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                        );
                        CREATE TABLE bucket_logs (
                                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                                        bucket_id INTEGER NOT NULL,
                                        log_id TEXT NOT NULL,
                                        timestamp TEXT NOT NULL,
                                        level TEXT NOT NULL,
                                        msg TEXT NOT NULL,
                                        props TEXT NOT NULL,
                                        created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                                        UNIQUE(bucket_id, log_id),
                                        FOREIGN KEY(bucket_id) REFERENCES log_buckets(id) ON DELETE CASCADE
                        );
                        CREATE INDEX IF NOT EXISTS bucket_logs_bucket_id_created_at_idx
                                ON bucket_logs(bucket_id, created_at DESC);
                "#,
	},
];

#[derive(Debug, Default)]
struct SqlitePragmaSetup;

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for SqlitePragmaSetup {
	fn on_acquire(
		&self,
		conn: &mut SqliteConnection,
	) -> std::result::Result<(), diesel::r2d2::Error> {
		conn.batch_execute(
			"PRAGMA journal_mode=WAL; PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON;",
		)
		.map_err(diesel::r2d2::Error::QueryError)
	}
}

type DbConn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

pub struct DbPools {
	pub write_pool: DbPool,
	pub read_pool: DbPool,
}

pub fn establish_pool(database_url: &str) -> Result<DbPool> {
	let manager = ConnectionManager::<SqliteConnection>::new(database_url);
	let mut builder = Pool::builder();
	if database_url == ":memory:" {
		builder = builder.max_size(1);
	} else {
		builder = builder.max_size(10);
	}
	builder
		.connection_customizer(Box::<SqlitePragmaSetup>::default())
		.build(manager)
		.context("failed to build sqlite pool")
}

pub fn open_db() -> DbPools {
	if cfg!(test) {
		let pool = establish_pool(":memory:").expect("in-memory pool");
		DbPools {
			write_pool: pool.clone(),
			read_pool: pool,
		}
	} else {
		let path = db_path();
		if let Some(parent) = path.parent() {
			if !parent.exists() {
				create_dir_all(parent).expect("failed to create database directory");
			}
		}
		let database_url = path.to_str().expect("database path is not utf-8");
		let write_pool = establish_pool(database_url).expect("pool");
		let read_pool = establish_pool(database_url).expect("pool");
		DbPools {
			write_pool,
			read_pool,
		}
	}
}

fn naive_to_utc(ts: NaiveDateTime) -> DateTime<Utc> {
	DateTime::<Utc>::from_utc(ts, Utc)
}

fn opt_naive_to_utc(ts: Option<NaiveDateTime>) -> Option<DateTime<Utc>> {
	ts.map(naive_to_utc)
}

#[derive(Queryable, Debug)]
#[diesel(table_name = devices)]
struct DeviceRow {
	id: String,
	send_logs: bool,
	filter_level: i32,
	logs_size: i64,
	logs_count: i64,
	created_at: NaiveDateTime,
	last_upload_at: Option<NaiveDateTime>,
	send_interval: i32,
}

#[derive(Queryable, Debug)]
#[diesel(table_name = log_segments)]
struct SegmentRow {
	id: i32,
	bucket_id: Option<i32>,
	device_id: Option<String>,
	first_timestamp: NaiveDateTime,
	last_timestamp: NaiveDateTime,
	original_size: i64,
	compressed_size: Option<i64>,
	logs_count: i64,
	created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = log_segments)]
struct NewSegmentRecord {
	bucket_id: Option<i32>,
	device_id: Option<String>,
	first_timestamp: NaiveDateTime,
	last_timestamp: NaiveDateTime,
	original_size: i64,
	compressed_size: Option<i64>,
	logs_count: i64,
}

#[derive(Queryable, Debug)]
#[diesel(table_name = log_buckets)]
struct LogBucketRow {
	id: i32,
	name: String,
	query: String,
	created_at: NaiveDateTime,
	updated_at: NaiveDateTime,
}

#[derive(Queryable, Debug)]
#[diesel(table_name = bucket_logs)]
struct BucketLogRow {
	id: i32,
	bucket_id: i32,
	log_id: String,
	timestamp: String,
	level: String,
	msg: String,
	props: String,
	created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = log_buckets)]
struct NewLogBucketRecord {
	name: String,
	query: String,
}

#[derive(Insertable)]
#[diesel(table_name = bucket_logs)]
struct NewBucketLogRecord {
	bucket_id: i32,
	log_id: String,
	timestamp: String,
	level: String,
	msg: String,
	props: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketProp {
	pub key: String,
	pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketLogEntry {
	pub id: String,
	pub timestamp: String,
	pub level: String,
	pub msg: String,
	pub props: Vec<BucketProp>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogBucket {
	pub id: i32,
	pub name: String,
	pub query: String,
	pub created_at: String,
	pub updated_at: String,
	pub logs: Vec<BucketLogEntry>,
}

#[derive(Debug, Clone)]
pub struct UpsertBucketArgs {
	pub id: Option<i32>,
	pub name: String,
	pub query: String,
}

#[derive(Debug, Clone)]
pub struct NewBucketLogEntry {
	pub id: String,
	pub timestamp: String,
	pub level: String,
	pub msg: String,
	pub props: Vec<BucketProp>,
}

const MAX_BUCKET_ENTRIES: usize = 200;
pub const BUCKET_LOG_LIMIT: usize = MAX_BUCKET_ENTRIES;

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

pub struct NewSegmentArgs {
	pub device_id: Option<String>,
	pub first_timestamp: chrono::DateTime<chrono::Utc>,
	pub last_timestamp: chrono::DateTime<chrono::Utc>,
	pub original_size: usize,
	pub compressed_size: usize,
	pub logs_count: u64,
}

#[derive(Debug)]
pub struct DB {
	write_pool: DbPool,
	read_pool: DbPool,
}

impl DB {
	pub fn new(pools: DbPools) -> Self {
		let DbPools {
			write_pool,
			read_pool,
		} = pools;
		{
			let mut conn = write_pool
				.get()
				.expect("failed to get connection for migrations");
			run_migrations(&mut conn).expect("failed to run migrations");
		}
		DB {
			write_pool,
			read_pool,
		}
	}

	fn conn(&self) -> Result<DbConn> {
		self.write_pool
			.get()
			.context("failed to get sqlite connection from pool")
	}

	fn read_conn(&self) -> Result<DbConn> {
		self.read_pool
			.get()
			.context("failed to get sqlite read connection from pool")
	}

	fn decode_bucket_log(row: &BucketLogRow) -> Option<BucketLogEntry> {
		if row.log_id.is_empty() || row.timestamp.is_empty() {
			return None;
		}
		let props: Vec<BucketProp> = serde_json::from_str(&row.props).unwrap_or_default();
		Some(BucketLogEntry {
			id: row.log_id.clone(),
			timestamp: row.timestamp.clone(),
			level: row.level.clone(),
			msg: row.msg.clone(),
			props,
		})
	}

	fn assemble_bucket(row: LogBucketRow, logs: Vec<BucketLogEntry>) -> LogBucket {
		LogBucket {
			id: row.id,
			name: row.name,
			query: row.query,
			created_at: naive_to_utc(row.created_at).to_rfc3339(),
			updated_at: naive_to_utc(row.updated_at).to_rfc3339(),
			logs,
		}
	}

	pub async fn list_buckets(&self) -> Result<Vec<LogBucket>> {
		let mut conn = self.read_conn()?;
		let rows: Vec<LogBucketRow> = log_buckets::table
			.order(log_buckets::updated_at.desc())
			.load(&mut conn)?;
		if rows.is_empty() {
			return Ok(Vec::new());
		}
		let ids: Vec<i32> = rows.iter().map(|row| row.id).collect();
		let log_rows: Vec<BucketLogRow> = bucket_logs::table
			.filter(bucket_logs::bucket_id.eq_any(&ids))
			.order((bucket_logs::bucket_id.asc(), bucket_logs::created_at.desc()))
			.load(&mut conn)?;
		let mut grouped: HashMap<i32, Vec<BucketLogEntry>> = HashMap::new();
		for row in log_rows {
			if let Some(entry) = Self::decode_bucket_log(&row) {
				grouped.entry(row.bucket_id).or_default().push(entry);
			}
		}
		let mut buckets = Vec::with_capacity(rows.len());
		for row in rows {
			let logs = grouped.remove(&row.id).unwrap_or_default();
			buckets.push(Self::assemble_bucket(row, logs));
		}
		Ok(buckets)
	}

	pub async fn get_bucket(&self, bucket_id: i32) -> Result<Option<LogBucket>> {
		let mut conn = self.read_conn()?;
		let bucket = log_buckets::table
			.filter(log_buckets::id.eq(bucket_id))
			.first::<LogBucketRow>(&mut conn)
			.optional()?;
		let Some(row) = bucket else {
			return Ok(None);
		};
		let log_rows: Vec<BucketLogRow> = bucket_logs::table
			.filter(bucket_logs::bucket_id.eq(bucket_id))
			.order(bucket_logs::created_at.desc())
			.load(&mut conn)?;
		let mut logs = Vec::with_capacity(log_rows.len());
		for log_row in log_rows.iter() {
			if let Some(entry) = Self::decode_bucket_log(log_row) {
				logs.push(entry);
			}
		}
		Ok(Some(Self::assemble_bucket(row, logs)))
	}

	pub async fn upsert_bucket(&self, args: UpsertBucketArgs) -> Result<LogBucket> {
		let UpsertBucketArgs { id, name, query } = args;
		let bucket_id = {
			let mut conn = self.conn()?;
			conn.transaction::<i32, diesel::result::Error, _>(|conn| {
				if let Some(existing_id) = id {
					let affected =
						diesel::update(log_buckets::table.filter(log_buckets::id.eq(existing_id)))
							.set((
								log_buckets::name.eq(&name),
								log_buckets::query.eq(&query),
								log_buckets::updated_at.eq(now),
							))
							.execute(conn)?;
					if affected > 0 {
						Ok(existing_id)
					} else {
						insert_into(log_buckets::table)
							.values(NewLogBucketRecord {
								name: name.clone(),
								query: query.clone(),
							})
							.execute(conn)?;
						let inserted_id = log_buckets::table
							.filter(log_buckets::name.eq(&name))
							.select(log_buckets::id)
							.first::<i32>(conn)?;
						Ok(inserted_id)
					}
				} else {
					insert_into(log_buckets::table)
						.values(NewLogBucketRecord {
							name: name.clone(),
							query: query.clone(),
						})
						.on_conflict(log_buckets::name)
						.do_update()
						.set((
							log_buckets::query.eq(&query),
							log_buckets::updated_at.eq(now),
						))
						.execute(conn)?;
					let inserted_id = log_buckets::table
						.filter(log_buckets::name.eq(&name))
						.select(log_buckets::id)
						.first::<i32>(conn)?;
					Ok(inserted_id)
				}
			})?
		};
		self.get_bucket(bucket_id)
			.await?
			.ok_or_else(|| anyhow::anyhow!("bucket not found after upsert"))
	}

	pub async fn append_bucket_logs(
		&self,
		bucket_id: i32,
		logs: &[NewBucketLogEntry],
	) -> Result<Option<LogBucket>> {
		if logs.is_empty() {
			return self.get_bucket(bucket_id).await;
		}
		let maybe_bucket = {
			let mut conn = self.conn()?;
			conn.transaction::<Option<i32>, diesel::result::Error, _>(|conn| {
				let exists = log_buckets::table
					.filter(log_buckets::id.eq(bucket_id))
					.select(log_buckets::id)
					.first::<i32>(conn)
					.optional()?;
				let Some(id) = exists else {
					return Ok(None);
				};
				let records: Vec<NewBucketLogRecord> = logs
					.iter()
					.map(|entry| NewBucketLogRecord {
						bucket_id: id,
						log_id: entry.id.clone(),
						timestamp: entry.timestamp.clone(),
						level: entry.level.clone(),
						msg: entry.msg.clone(),
						props: serde_json::to_string(&entry.props)
							.unwrap_or_else(|_| "[]".to_string()),
					})
					.collect();
				if !records.is_empty() {
					insert_or_ignore_into(bucket_logs::table)
						.values(&records)
						.execute(conn)?;
					let extra_ids: Vec<i32> = bucket_logs::table
						.select(bucket_logs::id)
						.filter(bucket_logs::bucket_id.eq(id))
						.order(bucket_logs::created_at.desc())
						.offset(MAX_BUCKET_ENTRIES as i64)
						.load::<i32>(conn)?;
					if !extra_ids.is_empty() {
						diesel::delete(
							bucket_logs::table.filter(bucket_logs::id.eq_any(extra_ids)),
						)
						.execute(conn)?;
					}
				}
				diesel::update(log_buckets::table.filter(log_buckets::id.eq(id)))
					.set(log_buckets::updated_at.eq(now))
					.execute(conn)?;
				Ok(Some(id))
			})?
		};
		match maybe_bucket {
			Some(id) => self.get_bucket(id).await,
			None => Ok(None),
		}
	}

	pub async fn clear_bucket_logs(&self, bucket_id: i32) -> Result<Option<LogBucket>> {
		let maybe_bucket = {
			let mut conn = self.conn()?;
			conn.transaction::<Option<i32>, diesel::result::Error, _>(|conn| {
				let exists = log_buckets::table
					.filter(log_buckets::id.eq(bucket_id))
					.select(log_buckets::id)
					.first::<i32>(conn)
					.optional()?;
				let Some(id) = exists else {
					return Ok(None);
				};
				diesel::delete(bucket_logs::table.filter(bucket_logs::bucket_id.eq(id)))
					.execute(conn)?;
				diesel::update(log_buckets::table.filter(log_buckets::id.eq(id)))
					.set(log_buckets::updated_at.eq(now))
					.execute(conn)?;
				Ok(Some(id))
			})?
		};
		match maybe_bucket {
			Some(id) => self.get_bucket(id).await,
			None => Ok(None),
		}
	}

	pub async fn delete_bucket(&self, bucket_id: i32) -> Result<bool> {
		let deleted = {
			let mut conn = self.conn()?;
			conn.transaction::<usize, diesel::result::Error, _>(|conn| {
				diesel::delete(bucket_logs::table.filter(bucket_logs::bucket_id.eq(bucket_id)))
					.execute(conn)?;
				diesel::delete(log_buckets::table.filter(log_buckets::id.eq(bucket_id)))
					.execute(conn)
			})?
		};
		Ok(deleted > 0)
	}

	pub async fn update_device_stats(
		&self,
		device_id: &str,
		logs_size: usize,
		logs_count: usize,
	) -> Result<()> {
		let mut conn = self.conn()?;
		diesel::sql_query(
			"INSERT INTO devices (id, logs_size, logs_count, last_upload_at) \
                         VALUES (?1, ?2, ?3, current_timestamp) \
                         ON CONFLICT(id) DO UPDATE SET \
                                logs_size = devices.logs_size + ?2, \
                                logs_count = devices.logs_count + ?3, \
                                last_upload_at = current_timestamp",
		)
		.bind::<Text, _>(device_id)
		.bind::<BigInt, _>(logs_size as i64)
		.bind::<BigInt, _>(logs_count as i64)
		.execute(&mut conn)
		.context("failed to update device stats")?;
		Ok(())
	}

	pub async fn get_devices(&self) -> Result<Vec<Device>> {
		let mut conn = self.read_conn()?;
		let rows: Vec<DeviceRow> = devices::table.load(&mut conn)?;
		let mut devices_vec = Vec::with_capacity(rows.len());
		for row in rows {
			let props = load_device_metadata(&mut conn, &row.id)?;
			devices_vec.push(Device {
				id: row.id,
				send_logs: row.send_logs,
				filter_level: LogLevel::from_i64(row.filter_level as i64),
				send_interval: row.send_interval as u32,
				logs_size: row.logs_size as usize,
				logs_count: row.logs_count as usize,
				created_at: naive_to_utc(row.created_at),
				last_upload_at: opt_naive_to_utc(row.last_upload_at),
				props,
			});
		}
		Ok(devices_vec)
	}

	pub async fn get_device(&self, device_id: &str) -> Result<Option<Device>> {
		let mut conn = self.read_conn()?;
		match devices::table
			.filter(devices::id.eq(device_id))
			.first::<DeviceRow>(&mut conn)
			.optional()?
		{
			Some(row) => {
				let props = load_device_metadata(&mut conn, &row.id)?;
				Ok(Some(Device {
					id: row.id,
					send_logs: row.send_logs,
					filter_level: LogLevel::from_i64(row.filter_level as i64),
					send_interval: row.send_interval as u32,
					logs_size: row.logs_size as usize,
					logs_count: row.logs_count as usize,
					created_at: naive_to_utc(row.created_at),
					last_upload_at: opt_naive_to_utc(row.last_upload_at),
					props,
				}))
			}
			None => Ok(None),
		}
	}

	pub async fn get_or_create_device(&self, device_id: &str) -> Result<Device> {
		let mut conn = self.conn()?;
		insert_or_ignore_into(devices::table)
			.values((
				devices::id.eq(device_id),
				devices::send_logs.eq(false),
				devices::filter_level.eq(LogLevel::Info.to_u8() as i32),
				devices::send_interval.eq(500),
				devices::logs_size.eq(0_i64),
				devices::logs_count.eq(0_i64),
				devices::last_upload_at.eq(Some(chrono::Utc::now().naive_utc())),
			))
			.execute(&mut conn)?;

		let row = devices::table
			.filter(devices::id.eq(device_id))
			.first::<DeviceRow>(&mut conn)?;

		let props = load_device_metadata(&mut conn, &row.id)?;
		Ok(Device {
			id: row.id,
			send_logs: row.send_logs,
			filter_level: LogLevel::from_i64(row.filter_level as i64),
			send_interval: row.send_interval as u32,
			logs_size: row.logs_size as usize,
			logs_count: row.logs_count as usize,
			created_at: naive_to_utc(row.created_at),
			last_upload_at: opt_naive_to_utc(row.last_upload_at),
			props,
		})
	}

	pub async fn update_device_settings(&self, device_id: &str, payload: &UpdateDeviceSettings) {
		let mut conn = self.conn().expect("failed to get connection");
		insert_into(devices::table)
			.values((
				devices::id.eq(device_id),
				devices::send_logs.eq(payload.send_logs),
				devices::filter_level.eq(payload.filter_level.to_u8() as i32),
				devices::send_interval.eq(payload.send_interval as i32),
			))
			.on_conflict(devices::id)
			.do_update()
			.set((
				devices::send_logs.eq(payload.send_logs),
				devices::filter_level.eq(payload.filter_level.to_u8() as i32),
				devices::send_interval.eq(payload.send_interval as i32),
			))
			.execute(&mut conn)
			.expect("failed to update device settings");
	}

	pub async fn update_device_metadata(
		&self,
		device_id: &str,
		metadata: &[MetaProp],
	) -> Result<()> {
		let mut conn = self.conn()?;
		conn.transaction::<_, diesel::result::Error, _>(|conn| {
			insert_or_ignore_into(devices::table)
				.values(devices::id.eq(device_id))
				.execute(conn)?;
			diesel::delete(device_props::table.filter(device_props::device_id.eq(device_id)))
				.execute(conn)?;
			for prop in metadata {
				insert_or_ignore_into(device_props::table)
					.values((
						device_props::device_id.eq(device_id),
						device_props::key.eq(&prop.key),
						device_props::value.eq(&prop.value),
					))
					.execute(conn)?;
			}
			Ok(())
		})?;
		Ok(())
	}

	pub async fn update_devices_settings(&self, payload: &UpdateDevicesSettings) -> Result<()> {
		let mut conn = self.conn()?;
		let mut filter: Option<Box<dyn BoxableExpression<devices::table, Sqlite, SqlType = Bool>>> =
			None;
		for prop in &payload.filter_props {
			let clause = exists(
				device_props::table
					.filter(device_props::device_id.eq(devices::id))
					.filter(device_props::key.eq(&prop.key))
					.filter(device_props::value.eq(&prop.value)),
			);
			filter = Some(match filter {
				Some(existing) => Box::new(existing.and(clause)),
				None => Box::new(clause),
			});
		}

		if let Some(condition) = filter {
			diesel::update(devices::table.filter(condition))
				.set((
					devices::send_logs.eq(payload.send_logs),
					devices::send_interval.eq(payload.send_interval as i32),
					devices::filter_level.eq(payload.level.to_u8() as i32),
				))
				.execute(&mut conn)?;
		} else {
			diesel::update(devices::table)
				.set((
					devices::send_logs.eq(payload.send_logs),
					devices::send_interval.eq(payload.send_interval as i32),
					devices::filter_level.eq(payload.level.to_u8() as i32),
				))
				.execute(&mut conn)?;
		}

		Ok(())
	}

	pub async fn new_segment(&self, args: NewSegmentArgs) -> Result<u32> {
		let mut conn = self.conn()?;
		let NewSegmentArgs {
			device_id,
			first_timestamp,
			last_timestamp,
			original_size,
			compressed_size,
			logs_count,
		} = args;
		let record = NewSegmentRecord {
			bucket_id: None,
			device_id,
			first_timestamp: first_timestamp.naive_utc(),
			last_timestamp: last_timestamp.naive_utc(),
			original_size: original_size as i64,
			compressed_size: Some(compressed_size as i64),
			logs_count: logs_count as i64,
		};

		conn.transaction::<_, diesel::result::Error, _>(|conn| {
			insert_into(log_segments::table)
				.values(&record)
				.execute(conn)?;
			let row: LastInsertRow =
				diesel::sql_query("SELECT last_insert_rowid() as id").get_result(conn)?;
			Ok(row.id)
		})
		.map(|id| id as u32)
		.map_err(Into::into)
	}

	pub async fn find_segments(&self, query: &GetSegmentsQuery) -> Result<Vec<SegmentMeta>> {
		let mut conn = self.read_conn()?;
		let mut q = log_segments::table.into_boxed();

		if let Some(start) = &query.start {
			q = q.filter(log_segments::last_timestamp.gt(start.naive_utc()));
		}
		if let Some(end) = &query.end {
			q = q.filter(log_segments::first_timestamp.le(end.naive_utc()));
		}
		if let Some(ids) = &query.device_ids {
			if ids.is_empty() {
				return Ok(Vec::new());
			}
			let filter_ids: Vec<Option<String>> = ids.iter().cloned().map(Some).collect();
			q = q.filter(log_segments::device_id.eq_any(filter_ids));
		}

		q = match query.sort {
			Some(SortDir::Asc) => q.order(log_segments::first_timestamp.asc()),
			Some(SortDir::Desc) => q.order(log_segments::first_timestamp.desc()),
			None => q.order(log_segments::id.asc()),
		};

		if let Some(count) = query.count {
			q = q.limit(count as i64);
		}

		let rows: Vec<SegmentRow> = q.load(&mut conn)?;
		Ok(rows
			.into_iter()
			.map(|row| SegmentMeta {
				id: row.id as u32,
				device_id: row.device_id,
				first_timestamp: naive_to_utc(row.first_timestamp),
				last_timestamp: naive_to_utc(row.last_timestamp),
				original_size: row.original_size as usize,
				compressed_size: row.compressed_size.unwrap_or(0) as usize,
				logs_count: row.logs_count as u64,
				created_at: naive_to_utc(row.created_at),
			})
			.collect())
	}

	pub async fn prev_segment_end(
		&self,
		ts: Option<&chrono::DateTime<chrono::Utc>>,
		device_ids: Option<&[String]>,
	) -> Result<Option<DateTime<Utc>>> {
		let mut conn = self.read_conn()?;
		let mut query = log_segments::table.into_boxed();
		if let Some(timestamp) = ts {
			query = query.filter(log_segments::last_timestamp.le(timestamp.naive_utc()));
		}
		if let Some(ids) = device_ids {
			if ids.is_empty() {
				return Ok(None);
			}
			let filter_ids: Vec<Option<String>> = ids.iter().cloned().map(Some).collect();
			query = query.filter(log_segments::device_id.eq_any(filter_ids));
		}
		let row: Option<NaiveDateTime> = query
			.select(log_segments::last_timestamp)
			.order(log_segments::last_timestamp.desc())
			.first::<NaiveDateTime>(&mut conn)
			.optional()?;
		Ok(row.map(naive_to_utc))
	}

	pub async fn segment_exists_at(
		&self,
		ts: chrono::DateTime<chrono::Utc>,
		device_ids: Option<&[String]>,
	) -> Result<bool> {
		let mut conn = self.read_conn()?;
		let mut subquery = log_segments::table
			.filter(log_segments::first_timestamp.le(ts.naive_utc()))
			.filter(log_segments::last_timestamp.ge(ts.naive_utc()))
			.into_boxed();

		if let Some(ids) = device_ids {
			if ids.is_empty() {
				return Ok(false);
			}
			let filter_ids: Vec<Option<String>> = ids.iter().cloned().map(Some).collect();
			subquery = subquery.filter(log_segments::device_id.eq_any(filter_ids));
		}

		let exists = diesel::select(exists(subquery)).get_result::<bool>(&mut conn)?;
		Ok(exists)
	}

	pub async fn fetch_segment(&self, segment: u32) -> Result<SegmentMeta> {
		let mut conn = self.read_conn()?;
		let row = log_segments::table
			.filter(log_segments::id.eq(segment as i32))
			.first::<SegmentRow>(&mut conn)?;
		Ok(SegmentMeta {
			id: row.id as u32,
			device_id: row.device_id,
			first_timestamp: naive_to_utc(row.first_timestamp),
			last_timestamp: naive_to_utc(row.last_timestamp),
			original_size: row.original_size as usize,
			compressed_size: row.compressed_size.unwrap_or(0) as usize,
			logs_count: row.logs_count as u64,
			created_at: naive_to_utc(row.created_at),
		})
	}

	pub async fn find_segments_without_device(
		&self,
		limit: Option<u32>,
	) -> Result<Vec<SegmentMeta>> {
		let mut conn = self.read_conn()?;
		let mut query = log_segments::table
			.filter(log_segments::device_id.is_null())
			.into_boxed();
		if let Some(limit) = limit {
			query = query.limit(limit as i64);
		}
		let rows: Vec<SegmentRow> = query.order(log_segments::id.asc()).load(&mut conn)?;
		Ok(rows
			.into_iter()
			.map(|row| SegmentMeta {
				id: row.id as u32,
				device_id: row.device_id,
				first_timestamp: naive_to_utc(row.first_timestamp),
				last_timestamp: naive_to_utc(row.last_timestamp),
				original_size: row.original_size as usize,
				compressed_size: row.compressed_size.unwrap_or(0) as usize,
				logs_count: row.logs_count as u64,
				created_at: naive_to_utc(row.created_at),
			})
			.collect())
	}

	pub async fn delete_segment(&self, segment: u32) -> Result<()> {
		let mut conn = self.conn()?;
		conn.transaction::<_, diesel::result::Error, _>(|conn| {
			diesel::delete(
				segment_props::table.filter(segment_props::segment_id.eq(segment as i32)),
			)
			.execute(conn)?;
			diesel::delete(log_segments::table.filter(log_segments::id.eq(segment as i32)))
				.execute(conn)?;
			Ok(())
		})?;
		Ok(())
	}

	pub async fn fetch_segments_metadata(&self) -> Result<SegmentsMetadata> {
		let mut conn = self.read_conn()?;
		#[derive(QueryableByName)]
		struct MetadataRow {
			#[diesel(sql_type = BigInt)]
			count: i64,
			#[diesel(sql_type = BigInt)]
			original_size: i64,
			#[diesel(sql_type = BigInt)]
			compressed_size: i64,
			#[diesel(sql_type = BigInt)]
			logs_count: i64,
		}
		let row = diesel::sql_query(
			"SELECT COUNT(*) as count, COALESCE(SUM(original_size), 0) as original_size, \
                         COALESCE(SUM(compressed_size), 0) as compressed_size, \
                         COALESCE(SUM(logs_count), 0) as logs_count FROM log_segments",
		)
		.get_result::<MetadataRow>(&mut conn)?;
		Ok(SegmentsMetadata {
			segment_count: row.count as u32,
			original_size: row.original_size as u64,
			compressed_size: row.compressed_size as u64,
			logs_count: row.logs_count as u64,
		})
	}

	pub async fn upsert_segment_props(
		&self,
		segment: u32,
		props: impl Iterator<Item = &Prop>,
	) -> Result<()> {
		let mut conn = self.conn()?;
		conn.transaction::<_, diesel::result::Error, _>(|conn| {
			for prop in props {
				insert_or_ignore_into(segment_props::table)
					.values((
						segment_props::segment_id.eq(segment as i32),
						segment_props::key.eq(&prop.key),
						segment_props::value.eq(&prop.value),
					))
					.execute(conn)?;
			}
			Ok(())
		})?;
		Ok(())
	}

	pub async fn fetch_segment_props(&self, segment: u32) -> Result<Vec<Prop>> {
		let mut conn = self.read_conn()?;
		let rows: Vec<(String, String)> = segment_props::table
			.filter(segment_props::segment_id.eq(segment as i32))
			.select((segment_props::key, segment_props::value))
			.load(&mut conn)?;
		Ok(rows
			.into_iter()
			.map(|(key, value)| Prop { key, value })
			.collect())
	}

	pub async fn fetch_segments_props(
		&self,
		segment_ids: &[u32],
	) -> Result<HashMap<u32, Vec<Prop>>> {
		const MAX_SQL_PARAMS: usize = 999;
		let mut conn = self.read_conn()?;
		if segment_ids.is_empty() {
			return Ok(HashMap::new());
		}
		let mut map: HashMap<u32, Vec<Prop>> = HashMap::new();
		for chunk in segment_ids.chunks(MAX_SQL_PARAMS) {
			let ids: Vec<i32> = chunk.iter().map(|id| *id as i32).collect();
			let rows: Vec<(i32, String, String)> = segment_props::table
				.filter(segment_props::segment_id.eq_any(&ids))
				.select((
					segment_props::segment_id,
					segment_props::key,
					segment_props::value,
				))
				.load(&mut conn)?;
			for (segment_id, key, value) in rows {
				map.entry(segment_id as u32)
					.or_default()
					.push(Prop { key, value });
			}
		}
		Ok(map)
	}
}

#[derive(QueryableByName)]
struct LastInsertRow {
	#[diesel(sql_type = BigInt)]
	id: i64,
}

fn load_device_metadata(conn: &mut SqliteConnection, device_id: &str) -> Result<Vec<MetaProp>> {
	let rows: Vec<(String, String)> = device_props::table
		.filter(device_props::device_id.eq(device_id))
		.order(device_props::key.asc())
		.select((device_props::key, device_props::value))
		.load(conn)?;
	Ok(rows
		.into_iter()
		.map(|(key, value)| MetaProp { key, value })
		.collect())
}

pub fn run_migrations(conn: &mut SqliteConnection) -> Result<()> {
	conn.batch_execute(
		"CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
	)?;

	#[derive(QueryableByName)]
	struct MigrationId {
		#[diesel(sql_type = Integer)]
		id: i32,
	}

	let applied: Vec<u32> = diesel::sql_query("SELECT id FROM migrations")
		.load::<MigrationId>(conn)?
		.into_iter()
		.map(|row| row.id as u32)
		.collect();

	let mut pending: Vec<&Migration> = MIGRATIONS
		.iter()
		.filter(|migration| !applied.contains(&migration.id))
		.collect();
	pending.sort_by_key(|migration| migration.id);

	for migration in pending {
		conn.transaction::<_, diesel::result::Error, _>(|conn| {
			conn.batch_execute(migration.sql)?;
			insert_into(migrations::table)
				.values((
					migrations::id.eq(migration.id as i32),
					migrations::name.eq(migration.name),
				))
				.execute(conn)?;
			Ok(())
		})?;
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use puppylog::Prop;
	use std::collections::HashSet;

	fn test_db() -> DB {
		let pool = establish_pool(":memory:").unwrap();
		DB::new(DbPools {
			write_pool: pool.clone(),
			read_pool: pool,
		})
	}

	#[tokio::test]
	async fn delete_segment_removes_props() {
		let db = test_db();

		let current_time = Utc::now();
		let segment = db
			.new_segment(NewSegmentArgs {
				device_id: None,
				first_timestamp: current_time,
				last_timestamp: current_time,
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
		let db = test_db();

		let current_time = Utc::now();
		let first_ts = current_time - chrono::Duration::hours(2);
		let last_ts = current_time - chrono::Duration::hours(1);
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

		let metas = db
			.find_segments(&GetSegmentsQuery {
				start: Some(current_time - chrono::Duration::minutes(90)),
				end: Some(current_time),
				device_ids: None,
				count: None,
				sort: None,
			})
			.await
			.unwrap();

		assert_eq!(metas.iter().map(|m| m.id).collect::<Vec<_>>(), vec![seg_id]);
	}

	#[tokio::test]
	async fn find_segments_without_device() {
		let db = test_db();
		let current_time = Utc::now();

		let no_dev = db
			.new_segment(NewSegmentArgs {
				device_id: None,
				first_timestamp: current_time,
				last_timestamp: current_time,
				original_size: 1,
				compressed_size: 1,
				logs_count: 1,
			})
			.await
			.unwrap();

		let _with_dev = db
			.new_segment(NewSegmentArgs {
				device_id: Some("dev1".into()),
				first_timestamp: current_time,
				last_timestamp: current_time,
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
		let db = test_db();
		let current_time = Utc::now();
		let ts_dev1 = current_time - chrono::Duration::hours(30);
		let ts_dev2 = current_time - chrono::Duration::hours(5);

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
			.prev_segment_end(Some(&current_time), Some(&["dev1".to_string()]))
			.await
			.unwrap()
			.unwrap();

		assert_eq!(found, ts_dev1);
	}

	#[tokio::test]
	async fn bucket_crud_flow() {
		let db = test_db();
		let bucket = db
			.upsert_bucket(UpsertBucketArgs {
				id: None,
				name: "Errors".into(),
				query: "level:error".into(),
			})
			.await
			.unwrap();
		assert_eq!(bucket.logs.len(), 0);

		let ts = Utc::now().to_rfc3339();
		let updated = db
			.append_bucket_logs(
				bucket.id,
				&[NewBucketLogEntry {
					id: "log-1".into(),
					timestamp: ts.clone(),
					level: "error".into(),
					msg: "something failed".into(),
					props: vec![BucketProp {
						key: "device".into(),
						value: "alpha".into(),
					}],
				}],
			)
			.await
			.unwrap()
			.expect("bucket exists");
		assert_eq!(updated.logs.len(), 1);
		assert_eq!(updated.logs[0].id, "log-1");

		let listed = db.list_buckets().await.unwrap();
		assert_eq!(listed.len(), 1);
		assert_eq!(listed[0].name, "Errors");
		assert_eq!(listed[0].logs.len(), 1);

		let cleared = db
			.clear_bucket_logs(bucket.id)
			.await
			.unwrap()
			.expect("bucket cleared");
		assert_eq!(cleared.logs.len(), 0);

		let deleted = db.delete_bucket(bucket.id).await.unwrap();
		assert!(deleted);
		assert!(db.list_buckets().await.unwrap().is_empty());
	}

	#[tokio::test]
	async fn bucket_log_limit_and_deduplication() {
		let db = test_db();
		let bucket = db
			.upsert_bucket(UpsertBucketArgs {
				id: None,
				name: "Recent".into(),
				query: "host:web".into(),
			})
			.await
			.unwrap();

		let mut entries = Vec::new();
		for i in 0..(BUCKET_LOG_LIMIT + 25) {
			entries.push(NewBucketLogEntry {
				id: format!("log-{i}"),
				timestamp: (Utc::now() + chrono::Duration::seconds(i as i64)).to_rfc3339(),
				level: "info".into(),
				msg: format!("message {i}"),
				props: vec![],
			});
		}

		db.append_bucket_logs(bucket.id, &entries)
			.await
			.unwrap()
			.expect("bucket exists");

		// Re-append first few entries to ensure they are deduplicated
		db.append_bucket_logs(bucket.id, &entries[..10])
			.await
			.unwrap()
			.expect("bucket exists");

		let refreshed = db.get_bucket(bucket.id).await.unwrap().unwrap();
		assert_eq!(refreshed.logs.len(), BUCKET_LOG_LIMIT);
		let unique: HashSet<&str> = refreshed.logs.iter().map(|log| log.id.as_str()).collect();
		assert_eq!(unique.len(), refreshed.logs.len());
	}
}
