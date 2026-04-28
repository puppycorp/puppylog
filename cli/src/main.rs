use chrono::NaiveDate;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use puppylog::{DrainParser, LogEntry, LogLevel, Prop};
use puppylog_server::{config::log_path, db, segment};
use rand::{distributions::Alphanumeric, prelude::*};
use reqwest::{self, Client, Url};
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::error::Error;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};
use std::time::Duration;
use tar::Archive;

const GITHUB_REPO: &str = "puppycorp/puppylog";
const UPDATE_CACHE_TTL_SECS: i64 = 24 * 60 * 60;
const BUILD_VERSION: &str = match option_env!("PLOG_BUILD_TAG") {
	Some(version) => version,
	None => "undefined",
};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct AuthConfig {
	token: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct PlogConfig {
	address: Option<String>,
	#[serde(default)]
	auth: AuthConfig,
}

/// Load default server URL if `--address` was not supplied on the command‑line.
/// Precedence:
/// 1. Environment variable `PUPPYLOG_ADDRESS`
/// 2. File `$HOME/.puppylog/config.json`
fn load_default_address() -> Option<String> {
	// env var first
	if let Ok(val) = std::env::var("PUPPYLOG_ADDRESS") {
		let trimmed = val.trim();
		if !trimmed.is_empty() {
			return Some(trimmed.to_owned());
		}
	}
	read_config().and_then(|config| config.address)
}

fn plog_home_dir() -> Option<std::path::PathBuf> {
	std::env::var("HOME")
		.ok()
		.map(std::path::PathBuf::from)
		.map(|home| home.join(".puppylog"))
}

fn config_path() -> Option<PathBuf> {
	plog_home_dir().map(|dir| dir.join("config.json"))
}

fn read_config() -> Option<PlogConfig> {
	let path = config_path()?;
	let content = std::fs::read_to_string(path).ok()?;
	serde_json::from_str(&content).ok()
}

fn write_config(config: &PlogConfig) -> Result<PathBuf, Box<dyn Error>> {
	let path = config_path().ok_or("HOME is not set")?;
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&path, serde_json::to_string_pretty(config)?)?;
	Ok(path)
}

fn load_default_auth_token() -> Option<String> {
	if let Ok(val) = std::env::var("PUPPYLOG_AUTH_TOKEN") {
		let trimmed = val.trim();
		if !trimmed.is_empty() {
			return Some(trimmed.to_owned());
		}
	}
	read_config().and_then(|config| config.auth.token)
}

fn masked_token(token: &str) -> String {
	if token.len() <= 4 {
		return "****".to_string();
	}
	format!("{}****", &token[..4])
}

fn apply_auth_header(request: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
	match token {
		Some(token) if !token.trim().is_empty() => request.header("Authorization", token),
		_ => request,
	}
}

fn update_cache_path() -> Option<std::path::PathBuf> {
	plog_home_dir().map(|dir| dir.join("plog-update-check.json"))
}

fn current_build_version() -> &'static str {
	BUILD_VERSION
}

fn release_api_url() -> String {
	format!(
		"https://api.github.com/repos/{}/releases/latest",
		GITHUB_REPO
	)
}

fn release_page_url(tag: &str) -> String {
	format!("https://github.com/{}/releases/tag/{}", GITHUB_REPO, tag)
}

fn platform_asset_name(tag: &str) -> Option<String> {
	match (std::env::consts::OS, std::env::consts::ARCH) {
		("linux", "x86_64") => Some(format!("plog-{tag}-x86_64-unknown-linux-gnu.tar.gz")),
		("macos", "aarch64") => Some(format!("plog-{tag}-aarch64-apple-darwin.tar.gz")),
		("windows", "x86_64") => Some(format!("plog-{tag}-x86_64-pc-windows-msvc.zip")),
		_ => None,
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum VersionPart {
	Number(u64),
	Text(String),
}

fn parse_version_parts(input: &str) -> Vec<VersionPart> {
	input
		.trim()
		.trim_start_matches(['v', 'V'])
		.split(['.', '-', '_'])
		.filter(|part| !part.is_empty())
		.map(|part| match part.parse::<u64>() {
			Ok(value) => VersionPart::Number(value),
			Err(_) => VersionPart::Text(part.to_ascii_lowercase()),
		})
		.collect()
}

fn compare_versions(left: &str, right: &str) -> CmpOrdering {
	let left_parts = parse_version_parts(left);
	let right_parts = parse_version_parts(right);
	let max_len = left_parts.len().max(right_parts.len());

	for idx in 0..max_len {
		let left_part = left_parts.get(idx);
		let right_part = right_parts.get(idx);
		let ordering = match (left_part, right_part) {
			(Some(VersionPart::Number(a)), Some(VersionPart::Number(b))) => a.cmp(b),
			(Some(VersionPart::Text(a)), Some(VersionPart::Text(b))) => a.cmp(b),
			(Some(VersionPart::Number(_)), Some(VersionPart::Text(_))) => CmpOrdering::Greater,
			(Some(VersionPart::Text(_)), Some(VersionPart::Number(_))) => CmpOrdering::Less,
			(Some(VersionPart::Number(a)), None) => a.cmp(&0),
			(Some(VersionPart::Text(_)), None) => CmpOrdering::Greater,
			(None, Some(VersionPart::Number(b))) => 0.cmp(b),
			(None, Some(VersionPart::Text(_))) => CmpOrdering::Less,
			(None, None) => CmpOrdering::Equal,
		};

		if ordering != CmpOrdering::Equal {
			return ordering;
		}
	}

	CmpOrdering::Equal
}

fn is_newer_version(latest: &str, current: &str) -> bool {
	compare_versions(latest, current) == CmpOrdering::Greater
}

#[derive(serde::Deserialize)]
struct GithubReleaseAsset {
	name: String,
	browser_download_url: String,
}

#[derive(serde::Deserialize)]
struct GithubRelease {
	tag_name: String,
	html_url: String,
	assets: Vec<GithubReleaseAsset>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UpdateCache {
	last_checked: i64,
	latest_tag: String,
	release_url: String,
}

async fn fetch_latest_release(client: &Client) -> Result<GithubRelease, Box<dyn Error>> {
	let release = client
		.get(release_api_url())
		.header(
			reqwest::header::USER_AGENT,
			format!("plog/{}", current_build_version()),
		)
		.header(reqwest::header::ACCEPT, "application/vnd.github+json")
		.send()
		.await?
		.error_for_status()?
		.json::<GithubRelease>()
		.await?;
	Ok(release)
}

fn read_update_cache() -> Option<UpdateCache> {
	let path = update_cache_path()?;
	let content = std::fs::read_to_string(path).ok()?;
	serde_json::from_str(&content).ok()
}

fn write_update_cache(cache: &UpdateCache) {
	let Some(path) = update_cache_path() else {
		return;
	};
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	if let Ok(content) = serde_json::to_string(cache) {
		let _ = std::fs::write(path, content);
	}
}

fn print_update_notice(latest_tag: &str, release_url: &str) {
	eprintln!(
		"update available: {} -> {} (run `plog update`)\n{}",
		current_build_version(),
		latest_tag,
		release_url
	);
}

async fn maybe_check_for_updates(client: &Client) {
	if std::env::var("PLOG_NO_UPDATE_CHECK").ok().as_deref() == Some("1") {
		return;
	}

	let now = Utc::now().timestamp();
	if let Some(cache) = read_update_cache() {
		if is_newer_version(&cache.latest_tag, current_build_version()) {
			print_update_notice(&cache.latest_tag, &cache.release_url);
		}
		if now - cache.last_checked < UPDATE_CACHE_TTL_SECS {
			return;
		}
	}

	let release =
		match tokio::time::timeout(Duration::from_secs(2), fetch_latest_release(client)).await {
			Ok(Ok(release)) => release,
			_ => return,
		};

	let cache = UpdateCache {
		last_checked: now,
		latest_tag: release.tag_name.clone(),
		release_url: release.html_url.clone(),
	};
	write_update_cache(&cache);

	if is_newer_version(&release.tag_name, current_build_version()) {
		print_update_notice(&release.tag_name, &release.html_url);
	}
}

fn extract_binary_from_tar_gz(
	archive_bytes: &[u8],
	binary_name: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
	let decoder = GzDecoder::new(Cursor::new(archive_bytes));
	let mut archive = Archive::new(decoder);
	for entry in archive.entries()? {
		let mut entry = entry?;
		let path = entry.path()?;
		if path.file_name().and_then(|name| name.to_str()) == Some(binary_name) {
			let mut bytes = Vec::new();
			entry.read_to_end(&mut bytes)?;
			return Ok(bytes);
		}
	}
	Err(format!("binary `{binary_name}` not found in archive").into())
}

fn extract_binary_from_zip(
	archive_bytes: &[u8],
	binary_name: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
	let reader = Cursor::new(archive_bytes);
	let mut archive = zip::ZipArchive::new(reader)?;
	for idx in 0..archive.len() {
		let mut file = archive.by_index(idx)?;
		if Path::new(file.name())
			.file_name()
			.and_then(|name| name.to_str())
			== Some(binary_name)
		{
			let mut bytes = Vec::new();
			file.read_to_end(&mut bytes)?;
			return Ok(bytes);
		}
	}
	Err(format!("binary `{binary_name}` not found in zip archive").into())
}

fn replace_current_binary(binary_bytes: &[u8]) -> Result<String, Box<dyn Error>> {
	let current_exe = std::env::current_exe()?;
	let file_name = current_exe
		.file_name()
		.and_then(|name| name.to_str())
		.ok_or("failed to determine current executable name")?;
	let temp_path = current_exe.with_extension("download");
	std::fs::write(&temp_path, binary_bytes)?;

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let mut perms = std::fs::metadata(&temp_path)?.permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(&temp_path, perms)?;
		std::fs::rename(&temp_path, &current_exe)?;
		return Ok(format!(
			"updated {} at {}",
			file_name,
			current_exe.display()
		));
	}

	#[cfg(windows)]
	{
		let staged_path =
			current_exe.with_file_name(format!("{}.new.exe", file_name.trim_end_matches(".exe")));
		std::fs::rename(&temp_path, &staged_path)?;
		return Ok(format!(
			"downloaded update to {} (close plog and replace the existing executable manually)",
			staged_path.display()
		));
	}

	#[cfg(not(any(unix, windows)))]
	{
		let _ = file_name;
		Err("self-update is not supported on this platform".into())
	}
}

async fn run_self_update(client: &Client) -> Result<(), Box<dyn Error>> {
	let release = fetch_latest_release(client).await?;
	if !is_newer_version(&release.tag_name, current_build_version()) {
		println!("plog is already up to date ({})", current_build_version());
		return Ok(());
	}

	let asset_name = platform_asset_name(&release.tag_name).ok_or_else(|| {
		format!(
			"no update asset is configured for {}-{}",
			std::env::consts::OS,
			std::env::consts::ARCH
		)
	})?;
	let asset = release
		.assets
		.iter()
		.find(|asset| asset.name == asset_name)
		.ok_or_else(|| {
			format!(
				"release {} does not contain asset {}",
				release.tag_name, asset_name
			)
		})?;

	println!("downloading {}", asset.name);
	let archive_bytes = client
		.get(&asset.browser_download_url)
		.header(
			reqwest::header::USER_AGENT,
			format!("plog/{}", current_build_version()),
		)
		.send()
		.await?
		.error_for_status()?
		.bytes()
		.await?;

	let binary_name = if cfg!(windows) { "plog.exe" } else { "plog" };
	let binary_bytes = if asset.name.ends_with(".tar.gz") {
		extract_binary_from_tar_gz(&archive_bytes, binary_name)?
	} else if asset.name.ends_with(".zip") {
		extract_binary_from_zip(&archive_bytes, binary_name)?
	} else {
		return Err(format!("unsupported update archive format: {}", asset.name).into());
	};

	let result = replace_current_binary(&binary_bytes)?;
	println!("{}", result);
	println!("updated to {}", release.tag_name);
	write_update_cache(&UpdateCache {
		last_checked: Utc::now().timestamp(),
		latest_tag: release.tag_name.clone(),
		release_url: release_page_url(&release.tag_name),
	});
	Ok(())
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
const API_NAMES: &[&str] = &["GetUser", "CreateOrder", "UpdateProfile", "DeleteAccount"];

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
#[command(author, version = BUILD_VERSION, about, long_about = None)]
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
enum LogsSubCommand {
	Download {
		#[arg(long)]
		count: u32,
		#[arg(long)]
		query: Option<String>,
		output: String,
	},
}

#[derive(Subcommand)]
enum ConfigSubCommand {
	SetAddress {
		address: String,
	},
	SetToken {
		token: String,
	},
	ClearToken,
	Show,
}

#[derive(Subcommand)]
enum Commands {
	/// Upload log data
	Upload(UploadLogsArgs),
	/// Download and install the latest plog release
	Update,
	/// Manage local CLI configuration in ~/.puppylog/config.json
	Config {
		#[command(subcommand)]
		subcommand: ConfigSubCommand,
	},
	Tokenize {
		#[command(subcommand)]
		subcommand: TokenizeSubcommands,
	},
	UpdateMetadata(UpdateMetadataArgs),
	#[command(subcommand)]
	Segment(SegmentSubCommand),
	#[command(subcommand)]
	Logs(LogsSubCommand),
	/// Import compressed log segments from a directory
	Import {
		folder: String,
	},
}

#[derive(Subcommand)]
enum TokenizeSubcommands {
	Drain { src: String, output: Option<String> },
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
		println!("importing {:?}", file_path.display());
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
				device_id: None,
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

#[derive(serde::Deserialize)]
struct DownloadedLogEntry {
	timestamp: String,
	level: String,
	#[serde(default)]
	props: Vec<DownloadedLogProp>,
	#[serde(alias = "message")]
	msg: String,
}

#[derive(serde::Deserialize)]
struct DownloadedLogProp {
	key: String,
	value: String,
}

fn format_download_timestamp(ts: &str) -> String {
	DateTime::parse_from_rfc3339(ts)
		.map(|date| date.format("%Y-%m-%d %H:%M:%S").to_string())
		.unwrap_or_else(|_| "unknown time".to_string())
}

fn day_start_utc(date: NaiveDate) -> DateTime<Utc> {
	date.and_hms_opt(0, 0, 0)
		.expect("valid start of day")
		.and_utc()
}

fn day_end_utc(date: NaiveDate) -> DateTime<Utc> {
	date.and_hms_opt(23, 59, 59)
		.expect("valid end of day")
		.and_utc()
}

fn format_download_line(entry: &DownloadedLogEntry) -> String {
	let props_text = if entry.props.is_empty() {
		String::new()
	} else {
		format!(
			" {}",
			entry
				.props
				.iter()
				.map(|prop| format!("{}={}", prop.key, prop.value))
				.collect::<Vec<_>>()
				.join(" ")
		)
	};
	let msg = entry.msg.replace(['\r', '\n'], " ").trim().to_string();
	let msg_text = if msg.is_empty() {
		String::new()
	} else {
		format!(" {}", msg)
	};
	format!(
		"{} {}{}{}",
		format_download_timestamp(&entry.timestamp),
		entry.level.to_uppercase(),
		props_text,
		msg_text
	)
}

async fn download_logs(
	client: &Client,
	base_addr: &str,
	auth_token: Option<&str>,
	count: u32,
	query: Option<String>,
	output: &str,
) -> Result<(), Box<dyn Error>> {
	let mut url = Url::parse(&format!("{}/api/logs", base_addr))?;
	{
		let mut params = url.query_pairs_mut();
		params.append_pair("count", &count.to_string());
		if let Some(query) = query.as_ref() {
			if !query.trim().is_empty() {
				params.append_pair("query", query);
			}
		}
	}

	println!("downloading logs: {}", url);
	let response = apply_auth_header(
		client.get(url).header(reqwest::header::ACCEPT, "application/json"),
		auth_token,
	)
	.send()
	.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(format!("download failed with {}: {}", status, body).into());
	}

	let entries = response.json::<Vec<DownloadedLogEntry>>().await?;
	let content = entries
		.iter()
		.map(format_download_line)
		.collect::<Vec<_>>()
		.join("\n");
	if let Some(parent) = Path::new(output).parent() {
		if !parent.as_os_str().is_empty() {
			std::fs::create_dir_all(parent)?;
		}
	}
	std::fs::write(output, content)?;
	println!("saved {} logs to {}", entries.len(), output);
	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let cli = Cli::parse();
	let update_client = reqwest::Client::new();

	if !matches!(&cli.subcommand, Commands::Update | Commands::Config { .. }) {
		maybe_check_for_updates(&update_client).await;
	}

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
		Commands::Update => {
			run_self_update(&update_client).await?;
		}
		Commands::Config { subcommand } => match subcommand {
			ConfigSubCommand::SetAddress { address } => {
				let mut config = read_config().unwrap_or_default();
				config.address = Some(address);
				let path = write_config(&config)?;
				println!("saved config to {}", path.display());
			}
			ConfigSubCommand::SetToken { token } => {
				let mut config = read_config().unwrap_or_default();
				config.auth.token = Some(token);
				let path = write_config(&config)?;
				println!("saved config to {}", path.display());
			}
			ConfigSubCommand::ClearToken => {
				let mut config = read_config().unwrap_or_default();
				config.auth.token = None;
				let path = write_config(&config)?;
				println!("saved config to {}", path.display());
			}
			ConfigSubCommand::Show => {
				let config = read_config().unwrap_or_default();
				let output = serde_json::json!({
					"address": config.address,
					"auth": {
						"token": config.auth.token.as_deref().map(masked_token),
					}
				});
				println!("{}", serde_json::to_string_pretty(&output)?);
			}
		},
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
			let auth_token = args.auth.clone().or_else(load_default_auth_token);
			let req = apply_auth_header(
				Client::new().post(&args.address),
				auth_token.as_deref(),
			)
				.header("Content-Type", "application/json")
				.body(props)
				.send()
				.await?;

			println!("Response: {:?}", req);
		}
		Commands::Segment(sub) => {
			let client = reqwest::Client::new();
			let base_addr = cli
				.address
				.clone()
				.or_else(load_default_address)
				.unwrap_or_else(|| "http://127.0.0.1:3337".to_string());
			let auth_token = load_default_auth_token();
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
							query.append_pair("start", &day_start_utc(start.unwrap()).to_string());
						}
						if end.is_some() {
							query.append_pair("end", &day_end_utc(end.unwrap()).to_string());
						}
						if count.is_some() {
							query.append_pair("count", &count.unwrap().to_string());
						}
						if sort.is_some() {
							query.append_pair("sort", &sort.unwrap());
						}
					}

					println!("URL: {}", url);

					let response = apply_auth_header(client.get(url), auth_token.as_deref())
						.send()
						.await?
						.text()
						.await?;
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
							query.append_pair("start", &day_start_utc(start.unwrap()).to_string());
						}
						if end.is_some() {
							query.append_pair("end", &day_end_utc(end.unwrap()).to_string());
						}
						if count.is_some() {
							query.append_pair("count", &count.unwrap().to_string());
						}
						if sort.is_some() {
							query.append_pair("sort", &sort.unwrap());
						}
					}

					let segements = apply_auth_header(client.get(url.clone()), auth_token.as_deref())
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
							let response = loop {
								let res = match apply_auth_header(client.get(url.clone()), auth_token.as_deref())
									.send()
									.await {
									Ok(res) => res,
									Err(e) => {
										eprintln!("Error downloading segment: {}", e);
										tokio::time::sleep(Duration::from_secs(1)).await;
										continue;
									}
								};

								match res.bytes().await {
									Ok(bytes) => break bytes,
									Err(e) => {
										eprintln!("Error downloading segment: {}", e);
										tokio::time::sleep(Duration::from_secs(1)).await;
									}
								}
							};
							println!("saving to file: {}", file_path.display());
							let mut file = std::fs::File::create(file_path)?;
							file.write_all(&response)?;
						}

						let url =
							Url::parse(&format!("{}/api/v1/segment/{}", base_addr, id)).unwrap();
						loop {
							let resp = match apply_auth_header(client.delete(url.clone()), auth_token.as_deref())
								.send()
								.await {
								Ok(r) => r,
								Err(e) => {
									eprintln!("Error deleting segment {}: {}", id, e);
									tokio::time::sleep(Duration::from_secs(1)).await;
									continue;
								}
							};
							if resp.status().is_success() {
								break;
							} else {
								eprintln!("Delete failed for segment {}: {}", id, resp.status());
								tokio::time::sleep(Duration::from_secs(1)).await;
							}
						}
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
							query.append_pair("start", &day_start_utc(start.unwrap()).to_string());
						}
						if end.is_some() {
							query.append_pair("end", &day_end_utc(end.unwrap()).to_string());
						}
						if count.is_some() {
							query.append_pair("count", &count.unwrap().to_string());
						}
						if sort.is_some() {
							query.append_pair("sort", &sort.unwrap());
						}
					}

					let segements = apply_auth_header(client.get(url.clone()), auth_token.as_deref())
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
						let response = apply_auth_header(client.get(url), auth_token.as_deref())
							.send()
							.await?
							.bytes()
							.await?;
						println!("saving to file: {}", file_path.display());
						let mut file = std::fs::File::create(file_path)?;
						file.write_all(&response)?;
					}
				}
			}
		}
		Commands::Logs(sub) => {
			let client = reqwest::Client::new();
			let base_addr = cli
				.address
				.clone()
				.or_else(load_default_address)
				.unwrap_or_else(|| "http://127.0.0.1:3337".to_string());
			let auth_token = load_default_auth_token();
			match sub {
				LogsSubCommand::Download {
					count,
					query,
					output,
				} => {
					download_logs(&client, &base_addr, auth_token.as_deref(), count, query, &output)
						.await?;
				}
			}
		}
		Commands::Import { folder } => {
			import_segments(&folder).await?;
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn format_download_line_flattens_props_and_newlines() {
		let entry = DownloadedLogEntry {
			timestamp: "2026-03-31T01:29:21.657081Z".to_string(),
			level: "error".to_string(),
			props: vec![DownloadedLogProp {
				key: "id".to_string(),
				value: "id-123".to_string(),
			}],
			msg: "line one\nline two".to_string(),
		};

		assert_eq!(
			format_download_line(&entry),
			"2026-03-31 01:29:21 ERROR id=id-123 line one line two"
		);
	}

	#[test]
	fn version_comparison_handles_numeric_tags() {
		assert!(is_newer_version("2", "1"));
		assert!(is_newer_version("2", "0.1.0"));
		assert!(is_newer_version("1.2.0", "1.1.9"));
		assert!(!is_newer_version("1", "2"));
		assert!(!is_newer_version("2", "2"));
	}

	#[test]
	fn masked_token_hides_secret_tail() {
		assert_eq!(masked_token("abcd1234"), "abcd****");
		assert_eq!(masked_token("abc"), "****");
	}
}
