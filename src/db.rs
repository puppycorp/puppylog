use std::fs::create_dir_all;

use anyhow::bail;
use puppylog::LogEntry;
use puppylog::LogLevel;
use puppylog::QueryAst;
use rusqlite::Connection;
use rusqlite::ToSql;
use tokio::sync::Mutex;

use crate::config::db_path;

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
			bytes_sent integer not null default 0,
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

	pub async fn search_logs(&self, query: QueryAst) -> anyhow::Result<Vec<LogEntry>> {
		let conn = self.conn.lock().await;
		let offset = query.offset.unwrap_or(0);
		let limit = query.limit.unwrap_or(200);
		let mut stmt = conn.prepare("SELECT random, timestamp, level, msg, props FROM logs order by timestamp desc limit 1000000 offset ?")?;
		let mut rows = stmt.query([offset])?;
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

    // Sort pending migrations by id to ensure correct order
    pending_migrations.sort_by_key(|migration| migration.id);
    if !pending_migrations.is_empty() {
        for migration in &pending_migrations {
            log::info!("applying migration {}: {}", migration.id, migration.name);

            // Begin a transaction for atomicity
            let tx = conn.transaction()?;

            // Execute the migration SQL
            tx.execute_batch(migration.sql)?;

            // Record the applied migration
            tx.execute(
                "INSERT INTO migrations (id, name) VALUES (?1, ?2)",
				&[&migration.id as &dyn ToSql, &migration.name as &dyn ToSql],
            )?;

            // Commit the transaction
            tx.commit()?;

            log::info!("migration {} applied successfully.", migration.id);
        }
    } else {
        log::info!("No new migrations to apply.");
    }

    Ok(())
}