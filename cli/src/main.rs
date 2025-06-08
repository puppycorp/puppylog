use chrono::NaiveDate;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use flate2::write::GzEncoder;
use flate2::Compression;
use log::Level;
use puppylog::{DrainParser, LogEntry, LogLevel, Prop, PuppylogBuilder};
use puppylog_server::{config::log_path, db, segment};
use rand::{distributions::Alphanumeric, prelude::*};
use reqwest::{self, Client, Url};
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::path::Path;
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};
use std::thread::sleep;
use std::time::Duration;

/// Load default server URL if `--address` was not supplied on the command‑line.
/// Precedence:
/// 1. Environment variable `PUPPYLOG_ADDRESS`
/// 2. File `$HOME/.puppylog/address` (first non‑empty line)
fn load_default_address() -> Option<String> {
	// env var first
	if let Ok(val) = std::env::var("PUPPYLOG_ADDRESS") {
		let trimmed = val.trim();
		if !trimmed.is_empty() {
			return Some(trimmed.to_owned());
		}
	}
	// config file
	if let Ok(home) = std::env::var("HOME") {
		let path = std::path::Path::new(&home)
			.join(".puppylog")
			.join("address");
		if let Ok(contents) = std::fs::read_to_string(&path) {
			let trimmed = contents.trim();
			if !trimmed.is_empty() {
				return Some(trimmed.to_owned());
			}
		}
	}
	None
}

// Constants from the Python version
const LOG_LEVELS: &[LogLevel] = &[
	LogLevel::Debug,
	LogLevel::Info,
	LogLevel::Warn,
	LogLevel::Error,
];
const LOG_LEVEL_WEIGHTS: &[f64] = &[5.0, 50.0, 30.0, 10.0, 5.0];

const ENTITY_TYPES: &[&str] = &[
	"instance",
	"user",
	"service",
	"device",
	"transaction",
	"task",
	"api request",
	"container",
	"node",
	"backup",
	"scheduler job",
	"email",
	"cache",
	"webhook",
	"database",
	"notification",
	"deployment",
	"license",
	"analytics event",
	"report",
	"session",
	"payment",
];

// Actions mapping - using a static HashMap via lazy_static
lazy_static::lazy_static! {
	static ref ACTIONS: HashMap<&'static str, Vec<&'static str>> = {
		let mut m = HashMap::new();
		m.insert("instance", vec!["created", "updated", "deleted"]);
		m.insert("user", vec!["registered", "logged in", "logged out unexpectedly"]);
		m.insert("service", vec!["started", "latency detected", "crashed"]);
		m.insert("device", vec!["connected", "signal weak", "disconnected"]);
		m.insert("transaction", vec!["initiated", "processed", "failed"]);
		m.insert("task", vec!["created", "running", "completed"]);
		m.insert("api request", vec!["initiated", "returned status", "failed"]);
		m.insert("container", vec!["started", "resource high", "crashed"]);
		m.insert("node", vec!["joined cluster", "under heavy load", "removed from cluster"]);
		m.insert("backup", vec!["started", "completed", "failed"]);
		m.insert("scheduler job", vec!["scheduled", "executing", "finished"]);
		m.insert("email", vec!["sent to", "delivery delayed to", "bounced from"]);
		m.insert("cache", vec!["cleared", "hit rate recorded", "updated"]);
		m.insert("webhook", vec!["received from", "processed successfully", "processing failed"]);
		m.insert("database", vec!["connection established", "query slow", "connection lost"]);
		m.insert("notification", vec!["queued for user", "delivered to user", "failed to deliver to user"]);
		m.insert("deployment", vec!["initiated by user", "in progress", "aborted due to error"]);
		m.insert("license", vec!["activated for user", "nearing expiration for user", "renewed for user"]);
		m.insert("analytics event", vec!["recorded for user", "processed", "failed to process"]);
		m.insert("report", vec!["generated for user", "downloaded by user", "generation failed for user"]);
		m.insert("session", vec!["started for user", "active", "inactive for too long"]);
		m.insert("payment", vec!["initiated by user", "authorized", "declined for user"]);
		m
	};
}

// Other constants
const STATUS_CODES: &[i32] = &[200, 201, 400, 401, 403, 404, 500, 502, 503];
const EMAIL_DOMAINS: &[&str] = &["example.com", "mail.com", "test.org", "sample.net"];
const SERVICE_NAMES: &[&str] = &[
	"AuthService",
	"DataService",
	"PaymentService",
	"NotificationService",
];
const DEVICE_NAMES: &[&str] = &["DeviceA", "DeviceB", "SensorX", "SensorY"];
const API_NAMES: &[&str] = &["GetUser", "CreateOrder", "UpdateProfile", "DeleteAccount"];
const DATABASE_NAMES: &[&str] = &["UserDB", "OrderDB", "AnalyticsDB", "InventoryDB"];
const WEBHOOK_SOURCES: &[&str] = &["GitHub", "Stripe", "Slack", "Twilio"];
const LICENSE_TYPES: &[&str] = &["Pro", "Enterprise", "Basic", "Premium"];
const REPORT_TYPES: &[&str] = &["Sales", "Inventory", "UserActivity", "Performance"];

// Helper functions to generate random IDs
fn generate_random_id(prefix: &str, length: usize) -> String {
	let rand_str: String = thread_rng()
		.sample_iter(&Alphanumeric)
		.take(length)
		.map(char::from)
		.collect();
	format!("{}-{}", prefix, rand_str)
}

fn random_string_name() -> String {
	let length = thread_rng().gen_range(5..11);
	thread_rng()
		.sample_iter(&Alphanumeric)
		.filter(|c| c.is_ascii_alphabetic())
		.take(length)
		.map(char::from)
		.collect()
}

fn random_email() -> String {
	let username: String = thread_rng()
		.sample_iter(&Alphanumeric)
		.take(7)
		.map(char::from)
		.collect();
	let domain = EMAIL_DOMAINS.choose(&mut thread_rng()).unwrap();
	format!("{}@{}", username.to_lowercase(), domain)
}

fn random_num() -> u32 {
	thread_rng().gen_range(1000..10000)
}

fn random_log_entry(timestamp: DateTime<Utc>) -> LogEntry {
	let mut rng = thread_rng();

	// Select log level using weights
	let level = LOG_LEVELS
		.choose_weighted(&mut rng, |&item| {
			LOG_LEVEL_WEIGHTS[LOG_LEVELS.iter().position(|&x| x == item).unwrap()]
		})
		.unwrap()
		.clone();

	let entity = *ENTITY_TYPES.choose(&mut rng).unwrap();
	let actions = ACTIONS.get(entity).unwrap();
	let action = *actions.choose(&mut rng).unwrap();

	// Generate the log line based on entity type
	let log_line = match entity {
		"user" => {
			let username = random_string_name();
			LogEntry {
				random: random_num(),
				timestamp,
				level,
				msg: format!("{} {} {}", entity, username, action),
				props: vec![Prop {
					key: "username".to_string(),
					value: username,
				}],
				..Default::default()
			}
		}
		"api request" => {
			let api_name = API_NAMES.choose(&mut rng).unwrap();
			if action == "returned status" {
				let status = STATUS_CODES.choose(&mut rng).unwrap();
				LogEntry {
					random: random_num(),
					timestamp,
					level,
					msg: format!("{} {} returned status {}", entity, api_name, status),
					props: vec![
						Prop {
							key: "api_name".to_string(),
							value: api_name.to_string(),
						},
						Prop {
							key: "status".to_string(),
							value: status.to_string(),
						},
					],
					..Default::default()
				}
			} else {
				LogEntry {
					random: random_num(),
					timestamp,
					level,
					msg: format!("{} {} {}", entity, api_name, action),
					props: vec![Prop {
						key: "api_name".to_string(),
						value: api_name.to_string(),
					}],
					..Default::default()
				}
			}
		}
		// Add similar matches for other entity types...
		_ => {
			let generic_id = generate_random_id("id", 8);
			LogEntry {
				random: random_num(),
				timestamp,
				level,
				msg: format!("{} {} {}", entity, generic_id, action),
				props: vec![Prop {
					key: "id".to_string(),
					value: generic_id,
				}],
				..Default::default()
			}
		}
	};

	log_line
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	/// Base URL of the puppylog server, e.g. http://127.0.0.1:3337
	#[arg(long)]
	address: Option<String>,

	#[command(subcommand)]
	subcommand: Commands,
}

#[derive(Parser)]
struct UpdateMetadataArgs {
	#[arg(long)]
	auth: Option<String>,
	#[arg(long)]
	address: String,
	props_path: String,
}

#[derive(Parser)]
struct UploadLogsArgs {
	#[arg(long)]
	address: String,
	#[arg(long)]
	count: u32,
	#[arg(long)]
	pararell: Option<u32>,
	#[arg(long)]
	auth: Option<String>,
}

#[derive(Subcommand)]
enum SegmentSubCommand {
	Get {
		#[arg(long)]
		start: Option<NaiveDate>,
		#[arg(long)]
		end: Option<NaiveDate>,
		#[arg(long)]
		count: Option<u32>,
		#[arg(long)]
		sort: Option<String>,
	},
	DownloadRemove {
		#[arg(long)]
		start: Option<NaiveDate>,
		#[arg(long)]
		end: Option<NaiveDate>,
		#[arg(long)]
		count: Option<u32>,
		#[arg(long)]
		sort: Option<String>,
		output: String,
	},
	Download {
		#[arg(long)]
		start: Option<NaiveDate>,
		#[arg(long)]
		end: Option<NaiveDate>,
		#[arg(long)]
		count: Option<u32>,
		#[arg(long)]
		sort: Option<String>,
		output: String,
	},
}

#[derive(Subcommand)]
enum Commands {
	/// Upload log data
	Upload(UploadLogsArgs),
	Tokenize {
		#[command(subcommand)]
		subcommand: TokenizeSubcommands,
	},
	UpdateMetadata(UpdateMetadataArgs),
	#[command(subcommand)]
	Segment(SegmentSubCommand),
	/// Import compressed log segments from a directory
	Import {
		folder: String,
	},
}

#[derive(Subcommand)]
enum TokenizeSubcommands {
	Drain { src: String, output: Option<String> },
}

async fn upload_logs(address: &str, logs: &[String], compress: bool) -> Result<(), Box<dyn Error>> {
	let client = reqwest::Client::new();
	let logs_str = logs.join("\n");

	let mut headers = reqwest::header::HeaderMap::new();

	let body = if compress {
		headers.insert(
			reqwest::header::CONTENT_ENCODING,
			reqwest::header::HeaderValue::from_static("gzip"),
		);

		let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
		encoder.write_all(logs_str.as_bytes())?;
		encoder.finish()?
	} else {
		logs_str.into_bytes()
	};

	let response = client
		.post(address)
		.headers(headers)
		.body(body)
		.send()
		.await?;

	println!("Upload status: {}", response.status());
	Ok(())
}

async fn import_segments(path: &str) -> anyhow::Result<()> {
	use std::collections::HashSet;
	use std::io::Cursor;
	use tokio::fs::{create_dir_all, read, read_dir};

	let log_dir = log_path();
	if !log_dir.exists() {
		create_dir_all(&log_dir).await?;
	}

	let db = db::DB::new(db::open_db());
	let mut dir = read_dir(path).await?;
	while let Some(entry) = dir.next_entry().await? {
		let file_path = entry.path();
		if !file_path.is_file() {
			continue;
		}

		let compressed = read(&file_path).await?;
		let compressed_size = compressed.len();
		let decoded = zstd::decode_all(Cursor::new(&compressed))?;
		let original_size = decoded.len();
		let mut cursor = Cursor::new(decoded);
		let segment = segment::LogSegment::parse(&mut cursor);
		if segment.buffer.is_empty() {
			continue;
		}
		let first_timestamp = segment.buffer.first().unwrap().timestamp;
		let last_timestamp = segment.buffer.last().unwrap().timestamp;
		let logs_count = segment.buffer.len() as u64;

		let segment_id = db
			.new_segment(db::NewSegmentArgs {
				first_timestamp,
				last_timestamp,
				original_size,
				compressed_size,
				logs_count,
			})
			.await?;

		let mut unique_props = HashSet::new();
		for log in &segment.buffer {
			for prop in &log.props {
				unique_props.insert(prop.clone());
			}
		}
		db.upsert_segment_props(segment_id, unique_props.iter())
			.await?;

		tokio::fs::write(log_dir.join(format!("{segment_id}.log")), &compressed).await?;
	}

	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let cli = Cli::parse();

	match cli.subcommand {
		Commands::Upload(args) => {
			let success_count = Arc::new(AtomicUsize::new(0));
			let fail_count = Arc::new(AtomicUsize::new(0));
			let mut handles = Vec::new();

			for i in 0..args.pararell.unwrap_or(1) {
				let addr = args.address.clone();
				println!("[{}] Uploading to {}", i, addr);
				let auth = args.auth.clone();
				let count = args.count;
				let success_count = Arc::clone(&success_count);
				let fail_count = Arc::clone(&fail_count);

				let handle = tokio::spawn(async move {
					let mut buffer = Vec::new();
					let mut timestamp = {
						let mut rng = thread_rng();
						// pick a random start up to 5 months (approx. 150 days) ago
						let max_secs = 5 * 30 * 24 * 3600;
						let offset_secs = rng.gen_range(0..=max_secs);
						Utc::now() - Duration::from_secs(offset_secs)
					};

					for _ in 0..count {
						let log = random_log_entry(timestamp);
						log.serialize(&mut buffer).unwrap();
						timestamp += Duration::from_millis(100);
					}

					let client = reqwest::Client::new();
					let response = client
						.post(&addr)
						.header("Authorization", auth.unwrap_or_default())
						.body(buffer)
						.send()
						.await
						.unwrap();

					if !response.status().is_success() {
						eprintln!("[{}] Upload failed: {}", i, response.status());
						fail_count.fetch_add(1, Ordering::SeqCst);
					} else {
						success_count.fetch_add(1, Ordering::SeqCst);
					}
				});
				handles.push(handle);
			}

			for handle in handles {
				if let Err(err) = handle.await {
					eprintln!("Error awaiting handle: {}", err);
				}
			}

			println!("Success count: {}", success_count.load(Ordering::SeqCst));
			println!("Fail count: {}", fail_count.load(Ordering::SeqCst));
		}
		Commands::Tokenize { subcommand } => {
			match subcommand {
				TokenizeSubcommands::Drain { src, output } => {
					let mut parser = DrainParser::new();
					parser.set_token_separators(vec![' ', ':', ',', ';']);
					//18:07:15,793
					//10.10.34.29:50010
					parser.set_wildcard_regex(r"(^[0-9]+$)|(^\d{4}-\d{2}-\d{2}$)|(^\d{2}:\d{2}:\d{2},\d{3}$)|(^/\d{1,3}.\d{1,3}.\d{1,3}.\d{1,3}:\d+$)");
					let logs = std::fs::read_to_string(src)?;
					let mut rows = vec!["TemplateID;Text".to_string()];
					let timer = std::time::Instant::now();
					for line in logs.lines() {
						parser.parse(line);
					}
					for (inx, line) in logs.lines().enumerate() {
						let template_id = parser.parse(line);
						let template_tokens = parser.get_template(template_id);
						//println!("Template ID: {} - {:?}", template_id, template_tokens.join(" "));
						rows.push(format!("{};{}", template_id, template_tokens.join(" ")));

						if inx % 1000 == 0 {
							let speed = inx as f64 / timer.elapsed().as_secs_f64();
							println!(
								"[{}] lines processed in {:?} templates count {} speed {:.2} l/s",
								inx,
								timer.elapsed(),
								parser.get_templates_count(),
								speed
							);
						}
					}
					if let Some(output) = output {
						std::fs::write(output, rows.join("\n"))?;
					}

					println!("Templates count: {}", parser.get_templates_count());
				}
			}
		}
		Commands::UpdateMetadata(args) => {
			let props = std::fs::read_to_string(args.props_path)?;
			println!("props: {}", props);
			let req = Client::new()
				.post(&args.address)
				.header("Authorization", args.auth.unwrap_or_default())
				.header("Content-Type", "application/json")
				.body(props)
				.send()
				.await?;

			println!("Response: {:?}", req);

			// let props: Vec<Prop> = serde_json::from_str(&props)?;
			// let logger = PuppylogBuilder::new()
			// 	.server(&address).unwrap()
			// 	.level(Level::Info)
			// 	.stdout()
			// 	.authorization("Bearer 123456")
			// 	.prop("app", "puppylogcli")
			// 	.build()
			// 	.unwrap();
			// logger.update_metadata(&device_id, props);
			// logger.close();
		}
		Commands::Segment(sub) => {
			let client = reqwest::Client::new();
			let base_addr = cli
				.address
				.clone()
				.or_else(load_default_address)
				.unwrap_or_else(|| "http://127.0.0.1:3337".to_string());
			match sub {
				SegmentSubCommand::Get {
					start,
					end,
					count,
					sort,
				} => {
					let mut url = Url::parse(&format!("{}/api/v1/segments", base_addr))?;
					{
						let mut query = url.query_pairs_mut();
						if start.is_some() {
							query.append_pair(
								"start",
								&start.unwrap().and_hms(0, 0, 0).and_utc().to_string(),
							);
						}
						if end.is_some() {
							query.append_pair(
								"end",
								&end.unwrap().and_hms(23, 59, 59).and_utc().to_string(),
							);
						}
						if count.is_some() {
							query.append_pair("count", &count.unwrap().to_string());
						}
						if sort.is_some() {
							query.append_pair("sort", &sort.unwrap());
						}
					}

					println!("URL: {}", url);

					let response = client.get(url).send().await?.text().await?;
					println!("Response: {:#?}", response);
				}
				SegmentSubCommand::DownloadRemove {
					start,
					end,
					count,
					sort,
					output,
				} => {
					let mut url = Url::parse(&format!("{}/api/v1/segments", base_addr))?;
					let output_path = Path::new(&output);
					if !output_path.exists() {
						std::fs::create_dir_all(output_path)?;
					}
					{
						let mut query = url.query_pairs_mut();
						if start.is_some() {
							query.append_pair(
								"start",
								&start.unwrap().and_hms(0, 0, 0).and_utc().to_string(),
							);
						}
						if end.is_some() {
							query.append_pair(
								"end",
								&end.unwrap().and_hms(23, 59, 59).and_utc().to_string(),
							);
						}
						if count.is_some() {
							query.append_pair("count", &count.unwrap().to_string());
						}
						if sort.is_some() {
							query.append_pair("sort", &sort.unwrap());
						}
					}

					let segements = client
						.get(url.clone())
						.send()
						.await?
						.json::<Vec<serde_json::Value>>()
						.await?;

					for segment in segements {
						let id = segment["id"].as_i64().unwrap().to_string();
						let url =
							Url::parse(&format!("{}/api/v1/segment/{}/download", base_addr, id))
								.unwrap();
						let file_path = output_path.join(format!("segment_{}.zstd", id));
						if !file_path.exists() {
							println!("downloading: {}", url);
							let response = client.get(url).send().await?.bytes().await?;
							println!("saving to file: {}", file_path.display());
							let mut file = std::fs::File::create(file_path)?;
							file.write_all(&response)?;
						}

						let url =
							Url::parse(&format!("{}/api/v1/segment/{}", base_addr, id)).unwrap();
						client.delete(url).send().await?;
						println!("deleted segment: {}", id);
					}
				}
				SegmentSubCommand::Download {
					start,
					end,
					count,
					sort,
					output,
				} => {
					let mut url = Url::parse(&format!("{}/api/v1/segments", base_addr))?;
					let output_path = Path::new(&output);
					if !output_path.exists() {
						std::fs::create_dir_all(output_path)?;
					}
					{
						let mut query = url.query_pairs_mut();
						if start.is_some() {
							query.append_pair(
								"start",
								&start.unwrap().and_hms(0, 0, 0).and_utc().to_string(),
							);
						}
						if end.is_some() {
							query.append_pair(
								"end",
								&end.unwrap().and_hms(23, 59, 59).and_utc().to_string(),
							);
						}
						if count.is_some() {
							query.append_pair("count", &count.unwrap().to_string());
						}
						if sort.is_some() {
							query.append_pair("sort", &sort.unwrap());
						}
					}

					let segements = client
						.get(url.clone())
						.send()
						.await?
						.json::<Vec<serde_json::Value>>()
						.await?;

					for segment in segements {
						let id = segment["id"].as_i64().unwrap().to_string();
						let url =
							Url::parse(&format!("{}/api/v1/segment/{}/download", base_addr, id))
								.unwrap();
						let file_path = output_path.join(format!("segment_{}.zst", id));
						if file_path.exists() {
							println!("file already exists: {}", file_path.display());
							continue;
						}

						println!("downloading: {}", url);
						let response = client.get(url).send().await?.bytes().await?;
						println!("saving to file: {}", file_path.display());
						let mut file = std::fs::File::create(file_path)?;
						file.write_all(&response)?;
					}
				}
			}
		}
		Commands::Import { folder } => {
			import_segments(&folder).await?;
		}
	}

	Ok(())
}
