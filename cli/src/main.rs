use chrono::{NaiveDate, TimeZone};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use log::Level;
use puppylog::{DrainParser, LogEntry, LogLevel, Prop, PuppylogBuilder};
use rand::{distributions::Alphanumeric, prelude::*};
use reqwest::{self, Client, RequestBuilder};
use std::collections::HashMap;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;
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
	let level = LOG_LEVELS.choose_weighted(&mut rng, |&item| {
		LOG_LEVEL_WEIGHTS[LOG_LEVELS.iter().position(|&x| x == item).unwrap()]
	}).unwrap().clone();
	
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
				props: vec![
					Prop {
						key: "username".to_string(),
						value: username
					}
				],
				..Default::default()
			}
		},
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
							value: api_name.to_string()
						},
						Prop {
							key: "status".to_string(),
							value: status.to_string()
						}
					],
					..Default::default()
				}
			} else {
				LogEntry {
					random: random_num(),
					timestamp,
					level,
					msg: format!("{} {} {}", entity, api_name, action),
					props: vec![
						Prop {
							key: "api_name".to_string(),
							value: api_name.to_string()
						}
					],
					..Default::default()
				}
			}
		},
		// Add similar matches for other entity types...
		_ => {
			let generic_id = generate_random_id("id", 8);
			LogEntry {
				random: random_num(),
				timestamp,
				level,
				msg: format!("{} {} {}", entity, generic_id, action),
				props: vec![
					Prop {
						key: "id".to_string(),
						value: generic_id
					}
				],
				..Default::default()
			}
		}
	};
	
	log_line
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    subcommand: Commands,
}

#[derive(Parser)]
struct StreamLogsArgs {
	#[arg(long)]
	address: String,
	#[arg(long)]
	interval: u64,
	#[arg(long)]
	count: Option<u64>,
	#[arg(long)]
	auth: Option<String>,
	#[arg(long, default_value = "1000")]
	increment: u32,
	#[arg(long)]
	start: Option<String>,
}

#[derive(Parser)]
struct UpdateMetadataArgs {
	#[arg(long)]
	auth: Option<String>,
	#[arg(long)]
	address: String,
	props_path: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Upload log data
    Upload {
        /// Server address
        address: String,
    },
	StreamLogs(StreamLogsArgs),
    Tokenize {
        #[command(subcommand)]
        subcommand: TokenizeSubcommands,
    },
	UpdateMetadata(UpdateMetadataArgs),
}

#[derive(Subcommand)]
enum TokenizeSubcommands {
    Drain {
        src: String,
        output: Option<String>,
    }
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
    let args = Cli::parse();
    
    // let logs = generate_logs(cli.count);

	// let client = reqwest::Client::new();

	// let mut buffer = Vec::new();
	// let mut cursor = std::io::Cursor::new(&mut buffer);
	// for log in logs {
	// 	log.serialize(&mut cursor)?;
	// }

    // let mut headers = reqwest::header::HeaderMap::new();

	// headers.insert(
	// 	reqwest::header::CONTENT_ENCODING,
	// 	reqwest::header::HeaderValue::from_static("gzip"),
	// );
	
	// let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
	// encoder.write_all(&buffer)?;
	// let body = encoder.finish()?;

    // let response = client
    //     .post(cli.address)
    //     .headers(headers)
    //     .body(body)
    //     .send()
    //     .await?;

    // println!("Upload status: {}", response.status());

    match args.subcommand {
		Commands::StreamLogs(args) => {
			let logger = PuppylogBuilder::new()
				.server(&args.address).unwrap()
				.level(Level::Info)
				.stdout()
				.authorization(&args.auth.unwrap_or_default())
				.prop("app", "puppylogcli")
				.build()
				.unwrap();

			fn parse_date_to_utc(date_str: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
				let naive_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
				let naive_datetime = naive_date.and_hms_opt(0, 0, 0).unwrap();
				Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc))
			}

			// Start in format YYYY-MM-DD
			let mut now = match args.start {
                Some(start) => parse_date_to_utc(&start).unwrap(),
				None => Utc::now()
			};

			let mut i = 0;
			loop {
				if i % 1000 == 0 {
					println!("[{}] timestamp: {}", i, now);
				}
				let log = random_log_entry(now);
				logger.send_logentry(log);
				now += Duration::from_millis(args.increment as u64);
				i += 1;
				if let Some(count) = args.count {
					if i >= count {
						break;
					}
				}
				sleep(Duration::from_millis(args.interval));
			}

			logger.close();
		},
        Commands::Upload { address } => todo!(),
        Commands::Tokenize { subcommand } => {
            match subcommand {
                TokenizeSubcommands::Drain { src, output } => {
                    let mut parser = DrainParser::new();
                    parser.set_token_separators(vec![' ', ':', ',', ';']);
                    //18:07:15,793
                    //10.10.34.29:50010 
                    parser.set_wildcard_regex(r"(^[0-9]+$)|(^\d{4}-\d{2}-\d{2}$)|(^\d{2}:\d{2}:\d{2},\d{3}$)|(^/\d{1,3}.\d{1,3}.\d{1,3}.\d{1,3}:\d+$)");
                    let logs = std::fs::read_to_string(src)?;
                    let mut rows = vec![
                        "TemplateID;Text".to_string(),
                    ];
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
                            println!("[{}] lines processed in {:?} templates count {} speed {:.2} l/s", inx, timer.elapsed(), parser.get_templates_count(), speed);
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
			let req = Client::new().post(&args.address)
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
    }
    
    Ok(())
}