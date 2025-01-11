use chrono::{DateTime, Duration, Utc};
use clap::{Parser, Subcommand};
use puppylog::{LogEntry, LogLevel};
use rand::{distributions::Alphanumeric, prelude::*};
use reqwest;
use std::collections::HashMap;
use std::error::Error;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

// Constants from the Python version
const LOG_LEVELS: &[LogLevel] = &[LogLevel::Debug, LogLevel::Info, LogLevel::Warn, LogLevel::Error];
const LOG_LEVEL_WEIGHTS: &[f64] = &[5.0, 50.0, 30.0, 10.0, 5.0];

const ENTITY_TYPES: &[&str] = &[
    "instance", "user", "service", "device", "transaction", "task", "api request",
    "container", "node", "backup", "scheduler job", "email", "cache",
    "webhook", "database", "notification", "deployment", "license",
    "analytics event", "report", "session", "payment"
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
const SERVICE_NAMES: &[&str] = &["AuthService", "DataService", "PaymentService", "NotificationService"];
const DEVICE_NAMES: &[&str] = &["DeviceA", "DeviceB", "SensorX", "SensorY"];
const API_NAMES: &[&str] = &["GetUser", "CreateOrder", "UpdateProfile", "DeleteAccount"];
const DATABASE_NAMES: &[&str] = &["UserDB", "OrderDB", "AnalyticsDB", "InventoryDB"];
const WEBHOOK_SOURCES: &[&str] = &["GitHub", "Stripe", "Slack", "Twilio"];
const LICENSE_TYPES: &[&str] = &["Pro", "Enterprise", "Basic", "Premium"];
const REPORT_TYPES: &[&str] = &["Sales", "Inventory", "UserActivity", "Performance"];

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Number of log lines to generate
    #[arg(short, long, default_value_t = 1000)]
    count: usize,
	#[arg(short, long, default_value_t = 0)]
	interval: u64,
	address: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Upload raw log data
    Rawupload {
        /// Server address
        address: String,
    },
    /// Stream raw log data
    Rawstream {
        /// Server address
        address: String,
    },
    /// Upload log data
    Upload {
        /// Server address
        address: String,
    },
}

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

fn random_timestamp(base_time: DateTime<Utc>) -> DateTime<Utc> {
    let offset = Duration::seconds(thread_rng().gen_range(0..100000));
    base_time + offset
}

fn generate_log_line(order: u32, base_time: DateTime<Utc>) -> LogEntry {
    let mut rng = thread_rng();
    
    // Select log level using weights
    let level = LOG_LEVELS.choose_weighted(&mut rng, |&item| {
        LOG_LEVEL_WEIGHTS[LOG_LEVELS.iter().position(|&x| x == item).unwrap()]
    }).unwrap().clone();
    
    let entity = *ENTITY_TYPES.choose(&mut rng).unwrap();
    let actions = ACTIONS.get(entity).unwrap();
    let action = *actions.choose(&mut rng).unwrap();
    
    let timestamp = random_timestamp(base_time);
	println!("timestamp: {:?}", timestamp);
    
    // Generate the log line based on entity type
    let log_line = match entity {
        "user" => {
            let username = random_string_name();
            // format!("{} {} {} {} {}", 
            //        timestamp.to_rfc3339(),
            //        log_level,
            //        entity,
            //        username,
            //        action);
			LogEntry {
				timestamp,
				level,
				msg: format!("{} {} {}", entity, username, action),
				props: vec![("username".to_string(), username)]
			}
        },
        "api request" => {
            let api_name = API_NAMES.choose(&mut rng).unwrap();
            if action == "returned status" {
                let status = STATUS_CODES.choose(&mut rng).unwrap();
                // format!("{} {} {} {} returned status {}", 
                //        timestamp.to_rfc3339(),
                //        log_level,
                //        entity,
                //        api_name,
                //        status)
				LogEntry {
					timestamp,
					level,
					msg: format!("{} {} returned status {}", entity, api_name, status),
					props: vec![("api_name".to_string(), api_name.to_string()), ("status".to_string(), status.to_string())]
				}
			} else {
                // format!("{} {} {} {} {}", 
                //        timestamp.to_rfc3339(),
                //        log_level,
                //        entity,
                //        api_name,
                //        action)
				LogEntry {
					timestamp,
					level,
					msg: format!("{} {} {}", entity, api_name, action),
					props: vec![("api_name".to_string(), api_name.to_string())]
				}
            }
        },
        // Add similar matches for other entity types...
        _ => {
            let generic_id = generate_random_id("id", 8);
            // format!("{} {} {} {} {}", 
            //        timestamp.to_rfc3339(),
            //        log_level,
            //        entity,
            //        generic_id,
            //        action)
			LogEntry {
				timestamp,
				level,
				msg: format!("{} {} {}", entity, generic_id, action),
				props: vec![("id".to_string(), generic_id)]
			}
        }
    };
    
    log_line
}

fn generate_logs(count: usize) -> Vec<LogEntry> {
    let base_time = Utc::now();
    (0..count)
        .map(|i| {
            let order = (i % 3 + 1) as u32;
            generate_log_line(order, base_time)
        })
        .collect()
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    
    let logs = generate_logs(cli.count);

	let client = reqwest::Client::new();

	let mut buffer = Vec::new();
	let mut cursor = std::io::Cursor::new(&mut buffer);
	for log in logs {
		log.serialize(&mut cursor)?;
	}

    let mut headers = reqwest::header::HeaderMap::new();

	headers.insert(
		reqwest::header::CONTENT_ENCODING,
		reqwest::header::HeaderValue::from_static("gzip"),
	);
	
	let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
	encoder.write_all(&buffer)?;
	let body = encoder.finish()?;

    let response = client
        .post(cli.address)
        .headers(headers)
        .body(body)
        .send()
        .await?;

    println!("Upload status: {}", response.status());
    
    Ok(())
}