use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use puppylog::{LogEntry, LogLevel, Prop};
use rand::distr::Alphanumeric;
use rand::{rng, Rng};
use reqwest::Client;
use serde::Deserialize;
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(
	author,
	version,
	about = "Simulate a Puppylog device sending logs to the server"
)]
struct Args {
	/// Base server URL (e.g. http://localhost:3337)
	#[arg(long, default_value = "http://localhost:3337")]
	server_url: String,
	/// Device identifier to use. Random if omitted.
	#[arg(long)]
	device_id: Option<String>,
	/// Number of log entries to send in each batch.
	#[arg(long, default_value_t = 100)]
	batch_size: usize,
	/// Override the send interval (seconds). When omitted the server recommendation is used.
	#[arg(long)]
	send_interval: Option<u64>,
	/// Optional upper bound on batches before exiting.
	#[arg(long)]
	max_batches: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeviceStatus {
	level: LogLevel,
	send_logs: bool,
	send_interval: u32,
	next_poll: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<()> {
	simple_logger::SimpleLogger::new()
		.with_level(log::LevelFilter::Info)
		.init()
		.ok();

	let args = Args::parse();
	let client = Client::new();
	let device_id = args.device_id.clone().unwrap_or_else(random_device_id);
	log::info!("starting simulator for device {}", device_id);

	let mut batches_sent = 0u64;
	let mut last_status: Option<DeviceStatus> = None;

	loop {
		if let Some(limit) = args.max_batches {
			if batches_sent >= limit {
				log::info!("max batches reached ({}), exiting", limit);
				break;
			}
		}

		match fetch_status(&client, &args.server_url, &device_id).await {
			Ok(status) => {
				log::debug!("device status: {:?}", status);
				last_status = Some(status);
			}
			Err(err) => {
				log::warn!("failed to fetch status: {}", err);
			}
		}

		let status = last_status.clone();
		if let Some(status) = &status {
			if !status.send_logs {
				log::info!(
					"server requested no log upload, waiting {} seconds",
					status.next_poll.unwrap_or(status.send_interval)
				);
				sleep(wait_duration(&args, status)).await;
				continue;
			}
		}

		let logs = generate_logs(args.batch_size, &device_id, status.as_ref());
		if logs.is_empty() {
			log::warn!("no logs generated for batch, waiting");
			sleep(wait_duration_from_status(&args, status.as_ref())).await;
			continue;
		}

		match send_logs(&client, &args.server_url, &device_id, &logs).await {
			Ok(sent) => {
				batches_sent += 1;
				log::info!("batch {}: uploaded {} entries", batches_sent, sent);
			}
			Err(err) => {
				log::error!("failed to upload logs: {}", err);
			}
		}

		sleep(wait_duration_from_status(&args, status.as_ref())).await;
	}

	Ok(())
}

fn wait_duration_from_status(args: &Args, status: Option<&DeviceStatus>) -> Duration {
	status
		.map(|s| wait_duration(args, s))
		.unwrap_or_else(|| Duration::from_secs(args.send_interval.unwrap_or(30)))
}

fn wait_duration(args: &Args, status: &DeviceStatus) -> Duration {
	let secs = args
		.send_interval
		.map(Duration::from_secs)
		.unwrap_or_else(|| {
			Duration::from_secs(status.next_poll.unwrap_or(status.send_interval) as u64)
		});
	if secs.is_zero() {
		Duration::from_secs(1)
	} else {
		secs
	}
}

async fn fetch_status(client: &Client, server_url: &str, device_id: &str) -> Result<DeviceStatus> {
	let url = format!(
		"{}/api/v1/device/{}/status",
		trimmed_base(server_url),
		urlencoding::encode(device_id)
	);
	client
		.get(url)
		.send()
		.await
		.context("status request failed")?
		.error_for_status()
		.context("status response status not ok")?
		.json::<DeviceStatus>()
		.await
		.context("status json parse failed")
}

async fn send_logs(
	client: &Client,
	server_url: &str,
	device_id: &str,
	logs: &[LogEntry],
) -> Result<usize> {
	let mut payload = Vec::with_capacity(logs.len() * 128);
	for entry in logs {
		entry
			.serialize(&mut payload)
			.map_err(|e| anyhow!("serialize log failed: {}", e))?;
	}

	let url = format!(
		"{}/api/v1/device/{}/logs",
		trimmed_base(server_url),
		urlencoding::encode(device_id)
	);
	let resp = client
		.post(url)
		.body(payload)
		.header("content-type", "application/octet-stream")
		.send()
		.await
		.context("log upload request failed")?;

	if !resp.status().is_success() {
		return Err(anyhow!("upload failed with status {}", resp.status()));
	}

	Ok(logs.len())
}

fn generate_logs(
	batch_size: usize,
	device_id: &str,
	status: Option<&DeviceStatus>,
) -> Vec<LogEntry> {
	let mut rng = rng();
	let mut entries = Vec::with_capacity(batch_size);
	let min_level = status.map(|s| s.level).unwrap_or(LogLevel::Info);

	for _ in 0..batch_size {
		let timestamp = Utc::now();
		let level = random_level(&mut rng, min_level);
		let (message, mut extra_props) = random_payload(&mut rng, device_id, timestamp, level);
		let firmware = format!(
			"{}.{:02}",
			1 + rng.random_range(0..3),
			rng.random_range(0..100)
		);
		let region = random_region(&mut rng);
		let mut props = vec![
			Prop {
				key: "deviceId".to_string(),
				value: device_id.to_string(),
			},
			Prop {
				key: "firmware".to_string(),
				value: firmware,
			},
			Prop {
				key: "region".to_string(),
				value: region,
			},
		];
		props.append(&mut extra_props);
		let entry = LogEntry {
			timestamp,
			random: rng.random::<u32>(),
			level,
			msg: message,
			props,
			..LogEntry::default()
		};
		entries.push(entry);
	}

	entries
}

fn random_payload<R: Rng>(
	rng: &mut R,
	device_id: &str,
	timestamp: DateTime<Utc>,
	level: LogLevel,
) -> (String, Vec<Prop>) {
	let choice = rng.random_range(0..100);
	if choice < 40 {
		return random_plain_payload(rng, device_id, timestamp, level);
	}
	if choice < 75 {
		return random_json_payload(rng, device_id, timestamp, level);
	}
	random_xml_payload(rng, device_id, timestamp, level)
}

fn random_plain_payload<R: Rng>(
	rng: &mut R,
	device_id: &str,
	timestamp: DateTime<Utc>,
	level: LogLevel,
) -> (String, Vec<Prop>) {
	let states = [
		"sensor array",
		"connection status",
		"battery system",
		"diagnostic suite",
		"telemetry stream",
		"heartbeat signal",
		"firmware watchdog",
	];
	let verbs = [
		"updated",
		"reported",
		"warned",
		"failed",
		"stabilized",
		"spiked",
	];
	let components = [
		"thermal-control",
		"network-module",
		"motor-driver",
		"camera-gimbal",
		"gps-receiver",
		"battery-pack",
		"lidar-sensor",
	];
	let anomalies = [
		"none",
		"voltage-drift",
		"signal-noise",
		"packet-loss",
		"calibration-slip",
		"firmware-mismatch",
	];
	let state = states[rng.random_range(0..states.len())];
	let verb = verbs[rng.random_range(0..verbs.len())];
	let component = components[rng.random_range(0..components.len())];
	let anomaly = anomalies[rng.random_range(0..anomalies.len())];
	let seq = rng.random_range(1_000..99_999);
	let context_len = rng.random_range(24..48);
	let context = random_ascii_blob(rng, context_len);
	let message = format!(
		"device {device_id} {verb} {state} on {component}; anomaly={anomaly}, sequence={seq}, ctx={context}, level={}, ts={}",
		level_name(level),
		timestamp.to_rfc3339()
	);
	let props = vec![
		Prop {
			key: "payloadFormat".to_string(),
			value: "text".to_string(),
		},
		Prop {
			key: "component".to_string(),
			value: component.to_string(),
		},
		Prop {
			key: "anomaly".to_string(),
			value: anomaly.to_string(),
		},
	];
	(message, props)
}

fn random_json_payload<R: Rng>(
	rng: &mut R,
	device_id: &str,
	timestamp: DateTime<Utc>,
	level: LogLevel,
) -> (String, Vec<Prop>) {
	let metric_count = rng.random_range(6..16);
	let statuses = ["ok", "warn", "error"];
	let units = ["celsius", "percent", "volt", "rpm", "psi", "lux"];
	let mut metrics = Vec::with_capacity(metric_count);
	for idx in 0..metric_count {
		let reading = rng.random_range(0..10_000) as f64 / 10.0;
		let status = statuses[rng.random_range(0..statuses.len())];
		let unit = units[rng.random_range(0..units.len())];
		let calibration_len = rng.random_range(8..24);
		let calibration = random_ascii_blob(rng, calibration_len);
		metrics.push(format!(
			"{{\"sensorId\":\"S{:02}\",\"reading\":{:.1},\"status\":\"{}\",\"unit\":\"{}\",\"calibration\":\"{}\"}}",
			idx,
			reading,
			status,
			unit,
			calibration
		));
	}
	let tag_options = [
		"environment",
		"diagnostic",
		"firmware",
		"connectivity",
		"profiling",
		"performance",
		"safety",
		"analytics",
	];
	let tag_count = rng.random_range(3..6);
	let mut tags: Vec<&'static str> = Vec::with_capacity(tag_count);
	for _ in 0..tag_count {
		let candidate = tag_options[rng.random_range(0..tag_options.len())];
		if !tags.contains(&candidate) {
			tags.push(candidate);
		}
	}
	if tags.is_empty() {
		tags.push("general");
	}
	let tag_str = tags
		.iter()
		.map(|tag| format!("\"{}\"", tag))
		.collect::<Vec<_>>()
		.join(",");
	let note_len = rng.random_range(180..360);
	let note = random_ascii_blob(rng, note_len);
	let batch_id = random_ascii_blob(rng, 12);
	let json = format!(
		"{{\"deviceId\":\"{}\",\"timestamp\":\"{}\",\"batchId\":\"{}\",\"level\":\"{}\",\"metrics\":[{}],\"tags\":[{}],\"notes\":\"{}\"}}",
		device_id,
		timestamp.to_rfc3339(),
		batch_id,
		level_name(level),
		metrics.join(","),
		tag_str,
		note
	);
	let props = vec![
		Prop {
			key: "payloadFormat".to_string(),
			value: "json".to_string(),
		},
		Prop {
			key: "payloadLength".to_string(),
			value: json.len().to_string(),
		},
		Prop {
			key: "batchId".to_string(),
			value: batch_id,
		},
	];
	(json, props)
}

fn random_xml_payload<R: Rng>(
	rng: &mut R,
	device_id: &str,
	timestamp: DateTime<Utc>,
	level: LogLevel,
) -> (String, Vec<Prop>) {
	let categories = [
		"telemetry",
		"diagnostic",
		"sensor",
		"network",
		"maintenance",
	];
	let category = categories[rng.random_range(0..categories.len())];
	let metric_count = rng.random_range(5..12);
	let units = ["C", "%", "V", "RPM", "Lux", "kPa"];
	let metric_entries = (0..metric_count)
		.map(|idx| {
			let reading = rng.random_range(0..10_000) as f64 / 10.0;
			let unit = units[rng.random_range(0..units.len())];
			format!(
				"<metric id=\"S{:02}\" unit=\"{}\">{:.1}</metric>",
				idx, unit, reading
			)
		})
		.collect::<Vec<_>>()
		.join("");
	let diag_count = rng.random_range(3..7);
	let diagnostic_entries = (0..diag_count)
		.map(|_| {
			let code = rng.random_range(1_000..9_999);
			let detail_len = rng.random_range(12..32);
			let detail = random_ascii_blob(rng, detail_len);
			format!("<diagnostic code=\"D{}\">{}</diagnostic>", code, detail)
		})
		.collect::<Vec<_>>()
		.join("");
	let payload_len = rng.random_range(220..420);
	let payload_blob = random_ascii_blob(rng, payload_len);
	let sequence = rng.random_range(10_000..999_999);
	let xml = format!(
		"<?xml version=\"1.0\" encoding=\"UTF-8\"?><logEntry deviceId=\"{}\" timestamp=\"{}\" level=\"{}\"><category>{}</category><sequence>{}</sequence><metrics>{}</metrics><diagnostics>{}</diagnostics><payload><![CDATA[{}]]></payload></logEntry>",
		device_id,
		timestamp.to_rfc3339(),
		level_name(level),
		category,
		sequence,
		metric_entries,
		diagnostic_entries,
		payload_blob
	);
	let props = vec![
		Prop {
			key: "payloadFormat".to_string(),
			value: "xml".to_string(),
		},
		Prop {
			key: "payloadLength".to_string(),
			value: xml.len().to_string(),
		},
		Prop {
			key: "sequence".to_string(),
			value: sequence.to_string(),
		},
		Prop {
			key: "category".to_string(),
			value: category.to_string(),
		},
	];
	(xml, props)
}

fn level_name(level: LogLevel) -> &'static str {
	match level {
		LogLevel::Trace => "trace",
		LogLevel::Debug => "debug",
		LogLevel::Info => "info",
		LogLevel::Warn => "warn",
		LogLevel::Error => "error",
		LogLevel::Fatal => "fatal",
		LogLevel::Uknown => "unknown",
	}
}

fn random_ascii_blob<R: Rng>(rng: &mut R, len: usize) -> String {
	rng.sample_iter(Alphanumeric)
		.take(len)
		.map(char::from)
		.collect()
}

fn random_region<R: Rng>(rng: &mut R) -> String {
	let regions = ["us-east", "us-west", "eu-central", "ap-south", "sa-east"];
	regions[rng.random_range(0..regions.len())].to_string()
}

fn random_level<R: Rng>(rng: &mut R, min_level: LogLevel) -> LogLevel {
	let levels = [
		LogLevel::Trace,
		LogLevel::Debug,
		LogLevel::Info,
		LogLevel::Warn,
		LogLevel::Error,
		LogLevel::Fatal,
	];
	let mut level = levels[rng.random_range(0..levels.len())];
	for _ in 0..levels.len() {
		if level >= min_level {
			return level;
		}
		level = levels[rng.random_range(0..levels.len())];
	}
	min_level
}

fn random_device_id() -> String {
	let generator = rng();
	let rand: String = generator
		.sample_iter(Alphanumeric)
		.take(8)
		.map(char::from)
		.collect();
	format!("device-{}", rand)
}

fn trimmed_base(server_url: &str) -> String {
	server_url.trim_end_matches('/').to_string()
}
