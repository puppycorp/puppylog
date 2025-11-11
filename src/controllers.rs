use std::sync::Arc;

use axum::body::{Body, BodyDataStream};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::Event;
use axum::response::{Html, IntoResponse, Response, Sse};
use axum::Json;
use chrono::{DateTime, Utc};
use futures::executor::block_on;
use futures::{future, Stream, StreamExt};
use puppylog::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string, Value};
use tokio::fs::{self, read_dir, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;

use crate::config::{log_path, upload_path};
use crate::context::{Context, LogStreamItem, SearchProgress, SegmentProgress};
use crate::db::{
	BucketProp, LogBucket, MetaProp, NewBucketLogEntry, UpdateDeviceSettings, UpsertBucketArgs,
	BUCKET_LOG_LIMIT,
};
use crate::types::GetSegmentsQuery;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetLogsQuery {
	pub count: Option<usize>,
	pub query: Option<String>,
	pub end_date: Option<DateTime<Utc>>,
	pub tz_offset: Option<i32>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetHistogramQuery {
	pub query: Option<String>,
	pub bucket_secs: Option<u64>,
	pub end_date: Option<DateTime<Utc>>,
	pub tz_offset: Option<i32>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpsertBucketRequest {
	pub id: Option<i32>,
	pub name: String,
	pub query: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BucketPropPayload {
	key: String,
	value: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BucketLogEntryPayload {
	id: String,
	timestamp: String,
	level: String,
	msg: String,
	#[serde(default)]
	props: Vec<BucketPropPayload>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppendBucketLogsRequest {
	#[serde(default)]
	logs: Vec<BucketLogEntryPayload>,
}

fn normalize_bucket_prop(prop: BucketPropPayload) -> Option<BucketProp> {
	let key = prop.key.trim();
	if key.is_empty() {
		return None;
	}
	Some(BucketProp {
		key: key.to_string(),
		value: prop.value.trim().to_string(),
	})
}

fn normalize_bucket_log(entry: BucketLogEntryPayload) -> Option<NewBucketLogEntry> {
	let id = entry.id.trim();
	let timestamp = entry.timestamp.trim();
	if id.is_empty() || timestamp.is_empty() {
		return None;
	}
	let level = match entry.level.as_str() {
		"trace" | "debug" | "info" | "warn" | "error" | "fatal" => entry.level,
		_ => return None,
	};
	let props = entry
		.props
		.into_iter()
		.filter_map(normalize_bucket_prop)
		.collect();
	Some(NewBucketLogEntry {
		id: id.to_string(),
		timestamp: timestamp.to_string(),
		level,
		msg: entry.msg,
		props,
	})
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfo {
	free_bytes: u64,
	total_bytes: u64,
	used_bytes: u64,
	used_percent: f64,
	upload_files_count: u64,
	upload_bytes: u64,
}

pub async fn get_server_info() -> Json<Value> {
	use crate::utility::disk_usage;

	let upload_dir = upload_path();

	// Disk usage (free and total for the filesystem hosting uploads path)
	let (free, total) = match disk_usage(&upload_dir) {
		Some(v) => v,
		None => (0, 0),
	};
	let used = total.saturating_sub(free);
	let used_percent = if total > 0 {
		(used as f64) / (total as f64) * 100.0
	} else {
		0.0
	};

	// Count files and sum bytes in the upload directory
	let mut upload_files_count: u64 = 0;
	let mut upload_bytes: u64 = 0;
	if upload_dir.exists() {
		if let Ok(mut dir) = read_dir(&upload_dir).await {
			while let Ok(Some(entry)) = dir.next_entry().await {
				let path = entry.path();
				if let Ok(meta) = entry.metadata().await {
					if meta.is_file() {
						upload_files_count = upload_files_count.saturating_add(1);
						upload_bytes = upload_bytes.saturating_add(meta.len());
					}
				} else if path.is_file() {
					// Fallback if metadata() fails but path suggests file
					upload_files_count = upload_files_count.saturating_add(1);
				}
			}
		}
	}

	Json(
		serde_json::to_value(ServerInfo {
			free_bytes: free,
			total_bytes: total,
			used_bytes: used,
			used_percent,
			upload_files_count,
			upload_bytes,
		})
		.unwrap(),
	)
}

pub async fn get_segment_metadata(State(ctx): State<Arc<Context>>) -> Json<Value> {
	let meta = ctx.db.fetch_segments_metadata().await.unwrap();
	let avg_logs_per_segment = meta.logs_count as f64 / meta.segment_count as f64;
	let avg_segment_size = meta.original_size as f64 / meta.segment_count as f64;
	Json(json!({
			"segmentCount": meta.segment_count,
			"originalSize": meta.original_size,
			"compressedSize": meta.compressed_size,
			"logsCount": meta.logs_count,
			"averageLogsPerSegment": avg_logs_per_segment,
			"averageSegmentSize": avg_segment_size
	}))
}

pub async fn list_buckets(
	State(ctx): State<Arc<Context>>,
) -> Result<Json<Vec<LogBucket>>, StatusCode> {
	ctx.db
		.list_buckets()
		.await
		.map(Json)
		.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn upsert_bucket(
	State(ctx): State<Arc<Context>>,
	Json(payload): Json<UpsertBucketRequest>,
) -> Result<Json<LogBucket>, StatusCode> {
	let name = payload.name.trim();
	if name.is_empty() {
		return Err(StatusCode::BAD_REQUEST);
	}
	let query = payload.query.unwrap_or_default();
	let bucket = ctx
		.db
		.upsert_bucket(UpsertBucketArgs {
			id: payload.id,
			name: name.to_string(),
			query,
		})
		.await
		.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
	if let Err(err) = ctx.refresh_bucket_filters().await {
		log::error!("failed to refresh bucket filters: {}", err);
	}
	Ok(Json(bucket))
}

pub async fn append_bucket_logs(
	State(ctx): State<Arc<Context>>,
	Path(bucket_id): Path<i32>,
	Json(payload): Json<AppendBucketLogsRequest>,
) -> Result<Json<LogBucket>, StatusCode> {
	let logs: Vec<NewBucketLogEntry> = payload
		.logs
		.into_iter()
		.filter_map(normalize_bucket_log)
		.take(BUCKET_LOG_LIMIT)
		.collect();
	match ctx.db.append_bucket_logs(bucket_id, &logs).await {
		Ok(Some(bucket)) => Ok(Json(bucket)),
		Ok(None) => Err(StatusCode::NOT_FOUND),
		Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
	}
}

pub async fn clear_bucket_logs(
	State(ctx): State<Arc<Context>>,
	Path(bucket_id): Path<i32>,
) -> Result<Json<LogBucket>, StatusCode> {
	match ctx.db.clear_bucket_logs(bucket_id).await {
		Ok(Some(bucket)) => Ok(Json(bucket)),
		Ok(None) => Err(StatusCode::NOT_FOUND),
		Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
	}
}

pub async fn delete_bucket(
	State(ctx): State<Arc<Context>>,
	Path(bucket_id): Path<i32>,
) -> StatusCode {
	match ctx.db.delete_bucket(bucket_id).await {
		Ok(true) => {
			if let Err(err) = ctx.refresh_bucket_filters().await {
				log::error!("failed to refresh bucket filters: {}", err);
			}
			StatusCode::NO_CONTENT
		}
		Ok(false) => StatusCode::NOT_FOUND,
		Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
	}
}

pub async fn get_segment(
	State(ctx): State<Arc<Context>>,
	Path(segment_id): Path<u32>,
) -> Json<Value> {
	let segment = ctx.db.fetch_segment(segment_id).await.unwrap();
	Json(serde_json::to_value(&segment).unwrap())
}

pub async fn download_segment(Path(segment_id): Path<u32>) -> Response {
	let path = log_path().join(format!("{segment_id}.log"));
	let file = match File::open(&path).await {
		Ok(f) => f,
		Err(e) => {
			return (
				StatusCode::NOT_FOUND,
				format!("segment {segment_id} not found: {e}"),
			)
				.into_response();
		}
	};

	let len = file.metadata().await.ok().map(|m| m.len());
	let stream = ReaderStream::new(BufReader::new(file));
	let body = Body::from_stream(stream);

	let mut headers = HeaderMap::new();
	headers.insert(header::CONTENT_TYPE, "application/zstd".parse().unwrap());
	headers.insert(
		header::CONTENT_DISPOSITION,
		format!(
			"attachment; filename=\"{}\"",
			format!("segment-{segment_id}.zst")
		)
		.parse()
		.unwrap(),
	);
	if let Some(len) = len {
		headers.insert(header::CONTENT_LENGTH, len.into());
	}

	(headers, body).into_response()
}

pub async fn delete_segment(
	State(ctx): State<Arc<Context>>,
	Path(segment_id): Path<u32>,
) -> &'static str {
	log::info!("delete_segment: {:?}", segment_id);
	ctx.db.delete_segment(segment_id).await.unwrap();
	let path = log_path().join(format!("{segment_id}.log"));
	if !path.exists() {
		log::warn!(
			"segment file {} does not exist, skipping deletion",
			path.display()
		);
		return "ok";
	}
	fs::remove_file(path).await.unwrap();
	"ok"
}

pub async fn get_segment_props(
	State(ctx): State<Arc<Context>>,
	Path(segment_id): Path<u32>,
) -> Json<Value> {
	let props = ctx.db.fetch_segment_props(segment_id).await.unwrap();
	Json(serde_json::to_value(&props).unwrap())
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BulkEdit {
	pub filter_level: LogLevel,
	pub send_logs: bool,
	pub send_interval: u32,
	pub device_ids: Vec<String>,
}

pub async fn bulk_edit(State(ctx): State<Arc<Context>>, body: Json<BulkEdit>) -> &'static str {
	log::info!("bulk_edit: {:?}", body);
	for device_id in body.device_ids.iter() {
		ctx.db
			.update_device_settings(
				device_id,
				&UpdateDeviceSettings {
					filter_level: body.filter_level,
					send_logs: body.send_logs,
					send_interval: body.send_interval,
				},
			)
			.await;
	}
	"ok"
}

pub async fn validate_query(Query(params): Query<GetLogsQuery>) -> Result<(), BadRequestError> {
	log::info!("validate_query {:?}", params);
	match params.query {
		Some(ref q) => {
			let q = q.replace('\n', " ");
			let q = q.trim();
			if q.is_empty() {
				return Ok(());
			}
			log::info!("validating query {}", q);
			if let Err(err) = parse_log_query(q) {
				log::error!("query {} is invalid: {}", q, err);
				return Err(BadRequestError(err.to_string()));
			}
			log::info!("query {} is valid", q);
		}
		None => {
			log::info!("query is empty");
		}
	};
	Ok(())
}

pub async fn update_device_settings(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
	body: Json<UpdateDeviceSettings>,
) -> &'static str {
	log::info!("update_device_settings device_id: {:?}", device_id);
	ctx.db.update_device_settings(&device_id, &body).await;
	"ok"
}

pub async fn get_devices(State(ctx): State<Arc<Context>>) -> Json<Value> {
	let devices = ctx.db.get_devices().await.unwrap();
	Json(serde_json::to_value(&devices).unwrap())
}

pub async fn get_device(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
	match ctx.db.get_device(&device_id).await {
		Ok(Some(device)) => Ok(Json(serde_json::to_value(&device).unwrap())),
		Ok(None) => Err(StatusCode::NOT_FOUND),
		Err(err) => {
			log::error!("get_device {} failed: {}", device_id, err);
			Err(StatusCode::INTERNAL_SERVER_ERROR)
		}
	}
}

pub async fn get_segments(
	State(ctx): State<Arc<Context>>,
	Query(mut params): Query<GetSegmentsQuery>,
) -> Json<Value> {
	if params.count.is_none() {
		params.count = Some(100);
	}
	let segments = ctx.db.find_segments(&params).await.unwrap();
	Json(serde_json::to_value(&segments).unwrap())
}

pub async fn upload_device_logs(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
	body: Body,
) -> impl IntoResponse {
	let _guard = match ctx.upload_guard() {
		Ok(g) => g,
		Err(err) => {
			let retry_after = rand::rng().random_range(10..=5_000);
			log::warn!("Upload guard busy: {}", err);
			let mut resp =
				(StatusCode::SERVICE_UNAVAILABLE, "Upload limit reached").into_response();
			resp.headers_mut().insert(
				axum::http::header::RETRY_AFTER,
				retry_after.to_string().parse().unwrap(),
			);
			return resp;
		}
	};

	let upload_dir = upload_path();
	let ts = chrono::Utc::now().timestamp_millis();
	let nonce: u32 = rand::rng().random_range(0..=u32::MAX);
	let part_path = upload_dir.join(format!("{device_id}-{ts:013}-{nonce:08x}.part"));

	// Create & open the temp file.
	let mut file = match OpenOptions::new()
		.create(true)
		.write(true)
		.truncate(true)
		.open(&part_path)
		.await
	{
		Ok(f) => f,
		Err(e) => {
			log::error!("cannot create {}: {}", part_path.display(), e);
			return (StatusCode::INTERNAL_SERVER_ERROR, "cannot create file").into_response();
		}
	};

	let mut stream: BodyDataStream = body.into_data_stream();
	while let Some(chunk) = stream.next().await {
		match chunk {
			Ok(bytes) => {
				if let Err(e) = file.write_all(&bytes).await {
					log::error!("write failed for {}: {}", part_path.display(), e);
					let _ = tokio::fs::remove_file(&part_path).await;
					return (StatusCode::INTERNAL_SERVER_ERROR, "write error").into_response();
				}
			}
			Err(e) => {
				log::error!("Error receiving chunk: {}", e);
				let _ = tokio::fs::remove_file(&part_path).await;
				return (StatusCode::BAD_REQUEST, "malformed upload").into_response();
			}
		}
	}

	if let Err(e) = file.sync_all().await {
		log::warn!("sync_all failed on {}: {}", part_path.display(), e);
	}
	drop(file); // ensure the handle is closed before rename

	let ready_path = part_path.with_extension("ready");
	if let Err(e) = tokio::fs::rename(&part_path, &ready_path).await {
		log::error!(
			"rename {} → {} failed: {}",
			part_path.display(),
			ready_path.display(),
			e
		);
		return (StatusCode::INTERNAL_SERVER_ERROR, "rename failed").into_response();
	}

	(StatusCode::OK, "ok").into_response()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceStatus {
	pub level: LogLevel,
	pub send_logs: bool,
	pub send_interval: u32,
	/// In how many seconds the device should poll next
	pub next_poll: Option<u32>,
}

pub async fn get_device_status(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
) -> Json<Value> {
	let device = match ctx.db.get_or_create_device(&device_id).await {
		Ok(device) => device,
		Err(err) => {
			log::error!("failed to get or create device {}: {}", device_id, err);
			return Json(Value::Null);
		}
	};

	let mut resp = DeviceStatus {
		level: device.filter_level,
		send_logs: device.send_logs,
		send_interval: device.send_interval,
		next_poll: None,
	};

	let allowed_to_send = ctx.allowed_to_upload();
	if !allowed_to_send {
		resp.send_logs = false;
		resp.next_poll = Some(rand::rng().random_range(10..=5000));
		log::info!(
			"[{}] not allowed to upload logs next poll {}",
			device_id,
			resp.next_poll.unwrap()
		);
	}

	Json(serde_json::to_value(resp).unwrap())
}

pub async fn update_device_metadata(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
	body: Json<Vec<MetaProp>>,
) -> &'static str {
	log::info!("update_device_metadata device_id: {:?}", device_id);
	ctx.db
		.update_device_metadata(&device_id, &body)
		.await
		.unwrap();
	"ok"
}

const INDEX_HTML: &str = include_str!("../assets/index.html");
const JS_HTML: &str = include_str!("../assets/puppylog.js");
const FAVICON: &[u8] = include_bytes!("../assets/favicon.ico");
const FAVICON_192x192: &[u8] = include_bytes!("../assets/favicon-192x192.png");
const FAVICON_512x512: &[u8] = include_bytes!("../assets/favicon-512x512.png");
const CSS: &str = include_str!("../assets/puppylog.css");

fn fnv1a64(bytes: &[u8]) -> u64 {
	const FNV_OFFSET: u64 = 0xcbf29ce484222325;
	const FNV_PRIME: u64 = 0x00000100000001B3;
	let mut hash = FNV_OFFSET;
	for b in bytes {
		hash ^= *b as u64;
		hash = hash.wrapping_mul(FNV_PRIME);
	}
	hash
}

fn etag_for(bytes: &[u8]) -> String {
	// Strong ETag using simple FNV-1a digest + length
	format!("\"{:016x}-{}\"", fnv1a64(bytes), bytes.len())
}

fn if_none_match_matches(etag: &str, candidate: &HeaderValue) -> bool {
	match candidate.to_str() {
		Ok(value) => value
			.split(',')
			.map(|part| part.trim())
			.any(|tag| tag == "*" || tag == etag || tag.strip_prefix("W/") == Some(etag)),
		Err(_) => false,
	}
}

fn cached_bytes_response(
	bytes: &'static [u8],
	content_type: &'static str,
	req_headers: &HeaderMap,
) -> Response {
	let etag = etag_for(bytes);
	if let Some(candidate) = req_headers.get(axum::http::header::IF_NONE_MATCH) {
		if if_none_match_matches(&etag, candidate) {
			let mut headers = HeaderMap::new();
			headers.insert(
				axum::http::header::ETAG,
				HeaderValue::from_str(&etag).unwrap(),
			);
			headers.insert(
				axum::http::header::CACHE_CONTROL,
				HeaderValue::from_static("public, max-age=31536000, immutable"),
			);
			headers.insert(
				axum::http::header::CONTENT_TYPE,
				HeaderValue::from_static(content_type),
			);
			return (StatusCode::NOT_MODIFIED, headers).into_response();
		}
	}

	let mut headers = HeaderMap::new();
	headers.insert(
		axum::http::header::ETAG,
		HeaderValue::from_str(&etag).unwrap(),
	);
	headers.insert(
		axum::http::header::CACHE_CONTROL,
		HeaderValue::from_static("public, max-age=31536000, immutable"),
	);
	headers.insert(
		axum::http::header::CONTENT_TYPE,
		HeaderValue::from_static(content_type),
	);
	(StatusCode::OK, headers, bytes).into_response()
}

fn cached_string_response(
	content: &str,
	content_type: &'static str,
	req_headers: &HeaderMap,
) -> Response {
	let bytes = content.as_bytes();
	let etag = etag_for(bytes);
	if let Some(candidate) = req_headers.get(axum::http::header::IF_NONE_MATCH) {
		if if_none_match_matches(&etag, candidate) {
			let mut headers = HeaderMap::new();
			headers.insert(
				axum::http::header::ETAG,
				HeaderValue::from_str(&etag).unwrap(),
			);
			headers.insert(
				axum::http::header::CACHE_CONTROL,
				HeaderValue::from_static("public, max-age=31536000, immutable"),
			);
			headers.insert(
				axum::http::header::CONTENT_TYPE,
				HeaderValue::from_static(content_type),
			);
			return (StatusCode::NOT_MODIFIED, headers).into_response();
		}
	}

	let mut headers = HeaderMap::new();
	headers.insert(
		axum::http::header::ETAG,
		HeaderValue::from_str(&etag).unwrap(),
	);
	headers.insert(
		axum::http::header::CACHE_CONTROL,
		HeaderValue::from_static("public, max-age=31536000, immutable"),
	);
	headers.insert(
		axum::http::header::CONTENT_TYPE,
		HeaderValue::from_static(content_type),
	);
	(StatusCode::OK, headers, content.to_string()).into_response()
}

#[cfg(debug_assertions)]
pub async fn css(headers: HeaderMap) -> Response {
	let mut file = tokio::fs::File::open("assets/puppylog.css").await.unwrap();
	let mut contents = String::new();
	file.read_to_string(&mut contents).await.unwrap();
	cached_string_response(&contents, "text/css; charset=utf-8", &headers)
}

#[cfg(not(debug_assertions))]
pub async fn css(headers: HeaderMap) -> Response {
	cached_string_response(CSS, "text/css; charset=utf-8", &headers)
}

// basic handler that responds with a static string
pub async fn root() -> Html<&'static str> {
	// Intentionally do not set long-lived cache for index.html
	Html(INDEX_HTML)
}

#[cfg(debug_assertions)]
pub async fn js(headers: HeaderMap) -> Response {
	let mut file = tokio::fs::File::open("assets/puppylog.js").await.unwrap();
	let mut contents = String::new();
	file.read_to_string(&mut contents).await.unwrap();
	cached_string_response(&contents, "application/javascript; charset=utf-8", &headers)
}

#[cfg(not(debug_assertions))]
pub async fn js(headers: HeaderMap) -> Response {
	cached_string_response(JS_HTML, "application/javascript; charset=utf-8", &headers)
}

#[derive(Deserialize, Debug)]
struct UpdateQuery {
	pub query: String,
}

pub async fn post_settings_query(State(ctx): State<Arc<Context>>, body: String) -> &'static str {
	log::info!("post_settings_query: {:?}", body);
	let mut settings = ctx.settings.inner().await;
	settings.collection_query = body.clone();
	settings.save().unwrap();
	ctx.event_tx
		.send(PuppylogEvent::QueryChanged { query: body })
		.unwrap();
	"ok"
}

pub async fn get_settings_query(State(ctx): State<Arc<Context>>) -> String {
	let settings = ctx.settings.inner().await;
	settings.collection_query.clone()
}

pub async fn favicon(headers: HeaderMap) -> Result<Response, StatusCode> {
	Ok(cached_bytes_response(FAVICON, "image/x-icon", &headers))
}

pub async fn favicon_192x192(headers: HeaderMap) -> Result<Response, StatusCode> {
	Ok(cached_bytes_response(
		FAVICON_192x192,
		"image/png",
		&headers,
	))
}

pub async fn favicon_512x512(headers: HeaderMap) -> Result<Response, StatusCode> {
	Ok(cached_bytes_response(
		FAVICON_512x512,
		"image/png",
		&headers,
	))
}

pub async fn manifest(headers: HeaderMap) -> Result<Response, StatusCode> {
	Ok(cached_bytes_response(
		include_bytes!("../assets/manifest.json"),
		"application/json",
		&headers,
	))
}

#[derive(Debug)]
pub(crate) struct BadRequestError(String);

impl IntoResponse for BadRequestError {
	fn into_response(self) -> Response {
		(
			StatusCode::BAD_REQUEST,
			Json(json!({
				"error": self.0
			})),
		)
			.into_response()
	}
}

fn logentry_to_json(entry: &LogEntry) -> Value {
	json!({
			"id": entry.id_string(),
			"version": entry.version,
			"timestamp": entry.timestamp,
			"level": entry.level.to_string(),
			"msg": entry.msg,
			"props": entry.props,
	})
}

fn segment_progress_to_json(progress: &SegmentProgress) -> Value {
	json!({
					"type": "segment",
					"segmentId": progress.segment_id,
					"deviceId": progress.device_id,
					"firstTimestamp": progress.first_timestamp,
					"lastTimestamp": progress.last_timestamp,
					"logsCount": progress.logs_count,
	})
}

fn search_progress_to_json(progress: &SearchProgress) -> Value {
	json!({
				"type": "stats",
				"processedLogs": progress.processed_logs,
				"logsPerSecond": progress.logs_per_second,
				"status": progress.status,
	})
}

pub async fn get_logs(
	State(ctx): State<Arc<Context>>,
	Query(params): Query<GetLogsQuery>,
	headers: HeaderMap,
) -> Result<Response, BadRequestError> {
	log::info!("get_logs {:?}", params);
	let mut query = match params.query {
		Some(ref q) => {
			let q = q.replace('\n', " ");
			let q = q.trim();
			if q.is_empty() {
				log::info!("query is empty");
				QueryAst::default()
			} else {
				match parse_log_query(q) {
					Ok(query) => query,
					Err(err) => return Err(BadRequestError(err.to_string())),
				}
			}
		}
		None => QueryAst::default(),
	};
	if let Some(offset) = params.tz_offset {
		query.tz_offset = chrono::FixedOffset::east_opt(-offset * 60);
	}
	query.limit = params.count;
	query.end_date = match params.end_date {
		Some(end_date) => Some(end_date),
		None => Some(Utc::now() + chrono::Duration::days(200)),
	};

	let (tx, rx) = mpsc::channel(100);
	let ctx_clone = Arc::clone(&ctx);
	let q = query.clone();
	spawn_blocking(move || {
		let _ = block_on(ctx_clone.find_logs(q, &tx));
	});

	let wants_stream = headers
		.get("accept")
		.and_then(|v| v.to_str().ok())
		.map(|s| s.contains("text/event-stream"))
		.unwrap_or(false);

	let limit = query.limit.unwrap_or(200) as usize;

	let res = if wants_stream {
		#[derive(Debug)]
		struct StreamState {
			entries_sent: usize,
			done: bool,
		}

		let stream = ReceiverStream::new(rx).scan(
			StreamState {
				entries_sent: 0,
				done: limit == 0,
			},
			move |state, item| {
				let limit = limit;
				if state.done {
					return future::ready(None);
				}
				match item {
					LogStreamItem::Entry(log) => {
						let data = to_string(&logentry_to_json(&log)).unwrap();
						state.entries_sent += 1;
						if state.entries_sent >= limit {
							state.done = true;
						}
						future::ready(Some(Ok::<Event, std::convert::Infallible>(
							Event::default().data(data),
						)))
					}
					LogStreamItem::SegmentProgress(progress) => {
						let data = to_string(&segment_progress_to_json(&progress)).unwrap();
						future::ready(Some(Ok::<Event, std::convert::Infallible>(
							Event::default().event("progress").data(data),
						)))
					}
					LogStreamItem::SearchProgress(progress) => {
						let data = to_string(&search_progress_to_json(&progress)).unwrap();
						future::ready(Some(Ok::<Event, std::convert::Infallible>(
							Event::default().event("progress").data(data),
						)))
					}
				}
			},
		);
		Sse::new(stream).into_response()
	} else {
		let logs: Vec<_> = ReceiverStream::new(rx)
			.filter_map(|item| async move {
				match item {
					LogStreamItem::Entry(log) => Some(logentry_to_json(&log)),
					LogStreamItem::SegmentProgress(_) | LogStreamItem::SearchProgress(_) => None,
				}
			})
			.take(limit)
			.collect()
			.await;
		Json(serde_json::to_value(&logs).unwrap()).into_response()
	};
	Ok(res)
}

pub async fn stream_logs(
	State(ctx): State<Arc<Context>>,
	Query(params): Query<GetLogsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, BadRequestError> {
	log::info!("stream logs {:?}", params);
	let mut query = match params.query {
		Some(ref query) => match parse_log_query(query) {
			Ok(query) => query,
			Err(err) => return Err(BadRequestError(err.to_string())),
		},
		None => QueryAst::default(),
	};
	if let Some(offset) = params.tz_offset {
		query.tz_offset = chrono::FixedOffset::east_opt(-offset * 60);
	}
	let rx = ctx.subscriber.subscribe(query).await;
	let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|p| {
		let data = to_string(&logentry_to_json(&p)).unwrap();
		Ok(Event::default().data(data))
	});
	Ok(Sse::new(stream))
}

pub async fn get_histogram(
	State(ctx): State<Arc<Context>>,
	Query(params): Query<GetHistogramQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, BadRequestError> {
	log::info!("get histogram {:?}", params);
	let bucket_secs = params.bucket_secs.unwrap_or(60);
	let mut query = match params.query {
		Some(ref q) => match parse_log_query(q) {
			Ok(q) => q,
			Err(err) => return Err(BadRequestError(err.to_string())),
		},
		None => QueryAst::default(),
	};
	if let Some(offset) = params.tz_offset {
		query.tz_offset = chrono::FixedOffset::east_opt(-offset * 60);
	}
	let (tx, rx) = mpsc::channel(100);
	let (entry_tx, mut entry_rx) = mpsc::channel(100);
	let ctx_clone = Arc::clone(&ctx);
	let q = query.clone();
	spawn_blocking(move || {
		let _ = block_on(ctx_clone.find_logs(q, &entry_tx));
	});

	tokio::spawn(async move {
		let mut current_bucket: Option<i64> = None;
		let mut count: u64 = 0;
		while let Some(item) = entry_rx.recv().await {
			let entry = match item {
				LogStreamItem::Entry(entry) => entry,
				LogStreamItem::SegmentProgress(_) | LogStreamItem::SearchProgress(_) => continue,
			};
			let ts = entry.timestamp.timestamp();
			let bucket = ts - ts % bucket_secs as i64;
			if let Some(cb) = current_bucket {
				if bucket != cb {
					if tx.is_closed() {
						break;
					}
					let item = json!({
					"timestamp": DateTime::<Utc>::from_timestamp(cb, 0).unwrap(),
					"count": count,
					});
					if tx.send(item).await.is_err() {
						break;
					}
					current_bucket = Some(bucket);
					count = 1;
				} else {
					count += 1;
				}
			} else {
				current_bucket = Some(bucket);
				count = 1;
			}
		}
		if let Some(cb) = current_bucket {
			let item = json!({
			"timestamp": DateTime::<Utc>::from_timestamp(cb, 0).unwrap(),
			"count": count,
			});
			let _ = tx.send(item).await;
		}
	});

	let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|item| {
		let data = to_string(&item).unwrap();
		Ok(Event::default().data(data))
	});
	Ok(Sse::new(stream))
}
#[cfg(test)]
mod tests {
	use super::*;
	use axum::body::{to_bytes, Body};
	use axum::http::{Request, StatusCode};
	use axum::routing::{delete, get, post};
	use axum::Router;
	use serde_json::{json, Value};
	use tempfile::TempDir;
	use tower::ServiceExt;

	#[tokio::test]
	#[serial_test::serial]
	async fn histogram_basic() {
		let dir = TempDir::new().unwrap();
		let log_dir = dir.path().join("logs");
		std::fs::create_dir_all(&log_dir).unwrap();
		std::env::set_var("LOG_PATH", &log_dir);
		std::env::set_var("DB_PATH", dir.path().join("db.sqlite"));
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));
		std::fs::write(
			dir.path().join("settings.json"),
			"{\"collection_query\":\"\"}",
		)
		.unwrap();

		let ctx = Arc::new(Context::new(log_dir).await);

		let base = Utc::now();
		ctx.save_logs(&[
			LogEntry {
				timestamp: base,
				msg: "a".into(),
				..Default::default()
			},
			LogEntry {
				timestamp: base + chrono::Duration::seconds(10),
				msg: "b".into(),
				..Default::default()
			},
			LogEntry {
				timestamp: base + chrono::Duration::seconds(70),
				msg: "c".into(),
				..Default::default()
			},
		])
		.await;

		let app = Router::new()
			.route("/api/v1/logs/histogram", get(get_histogram))
			.with_state(ctx);

		let res = app
			.oneshot(
				Request::builder()
					.uri("/api/v1/logs/histogram?bucketSecs=60")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res.status(), StatusCode::OK);
		let ct = res.headers().get(axum::http::header::CONTENT_TYPE).unwrap();
		assert_eq!(ct, "text/event-stream");
	}

	#[tokio::test]
	async fn validate_query_allows_newlines() {
		let dir = TempDir::new().unwrap();
		let log_dir = dir.path().join("logs");
		std::fs::create_dir_all(&log_dir).unwrap();
		std::env::set_var("LOG_PATH", &log_dir);
		std::env::set_var("DB_PATH", dir.path().join("db.sqlite"));
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));
		std::fs::write(
			dir.path().join("settings.json"),
			"{\"collection_query\":\"\"}",
		)
		.unwrap();

		let ctx = Arc::new(Context::new(log_dir).await);
		let app = Router::new()
			.route("/api/v1/validate_query", get(validate_query))
			.with_state(ctx);

		let encoded = "level%20=%20info%0Aor%20level%20=%20error";
		let res = app
			.oneshot(
				Request::builder()
					.uri(format!("/api/v1/validate_query?query={}", encoded))
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(res.status(), StatusCode::OK);
	}

	#[tokio::test]
	async fn get_logs_handles_newlines() {
		let dir = TempDir::new().unwrap();
		let log_dir = dir.path().join("logs");
		std::fs::create_dir_all(&log_dir).unwrap();
		std::env::set_var("LOG_PATH", &log_dir);
		std::env::set_var("DB_PATH", dir.path().join("db.sqlite"));
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));
		std::fs::write(
			dir.path().join("settings.json"),
			"{\"collection_query\":\"\"}",
		)
		.unwrap();

		let ctx = Arc::new(Context::new(log_dir).await);

		let base = Utc::now();
		ctx.save_logs(&[
			LogEntry {
				timestamp: base,
				level: puppylog::LogLevel::Info,
				msg: "a".into(),
				..Default::default()
			},
			LogEntry {
				timestamp: base + chrono::Duration::seconds(1),
				level: puppylog::LogLevel::Error,
				msg: "b".into(),
				..Default::default()
			},
		])
		.await;

		let app = Router::new()
			.route("/api/logs", get(get_logs))
			.with_state(ctx);

		let encoded = "level%20=%20info%0Aor%20level%20=%20error";
		let res = app
			.oneshot(
				Request::builder()
					.uri(format!("/api/logs?count=10&query={}", encoded))
					.header("accept", "application/json")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert_eq!(res.status(), StatusCode::OK);
		let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
		let logs: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
		assert_eq!(logs.len(), 2);
	}

	#[tokio::test]
	async fn bucket_endpoints_roundtrip() {
		let dir = TempDir::new().unwrap();
		let log_dir = dir.path().join("logs");
		std::fs::create_dir_all(&log_dir).unwrap();
		std::env::set_var("LOG_PATH", &log_dir);
		std::env::set_var("DB_PATH", dir.path().join("db.sqlite"));
		std::env::set_var("SETTINGS_PATH", dir.path().join("settings.json"));
		std::fs::write(
			dir.path().join("settings.json"),
			"{\"collection_query\":\"\"}",
		)
		.unwrap();

		let ctx = Arc::new(Context::new(log_dir).await);

		let app = Router::new()
			.route("/api/v1/buckets", get(list_buckets).post(upsert_bucket))
			.route("/api/v1/buckets/{bucketId}/logs", post(append_bucket_logs))
			.route("/api/v1/buckets/{bucketId}/clear", post(clear_bucket_logs))
			.route("/api/v1/buckets/{bucketId}", delete(delete_bucket))
			.with_state(ctx);

		let create_res = app
			.clone()
			.oneshot(
				Request::builder()
					.method("POST")
					.uri("/api/v1/buckets")
					.header("content-type", "application/json")
					.body(Body::from(
						json!({
								"name": "Errors",
								"query": "level:error"
						})
						.to_string(),
					))
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(create_res.status(), StatusCode::OK);
		let body = to_bytes(create_res.into_body(), usize::MAX).await.unwrap();
		let created: Value = serde_json::from_slice(&body).unwrap();
		let bucket_id = created["id"].as_i64().unwrap() as i32;

		let list_res = app
			.clone()
			.oneshot(
				Request::builder()
					.uri("/api/v1/buckets")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(list_res.status(), StatusCode::OK);
		let list_body = to_bytes(list_res.into_body(), usize::MAX).await.unwrap();
		let listed: Value = serde_json::from_slice(&list_body).unwrap();
		assert_eq!(listed.as_array().unwrap().len(), 1);

		let append_payload = json!({
				"logs": [
						{
								"id": "log-1",
								"timestamp": Utc::now().to_rfc3339(),
								"level": "info",
								"msg": "hello",
								"props": [{"key": "device", "value": "alpha"}]
						}
				]
		});
		let append_res = app
			.clone()
			.oneshot(
				Request::builder()
					.method("POST")
					.uri(format!("/api/v1/buckets/{bucket_id}/logs"))
					.header("content-type", "application/json")
					.body(Body::from(append_payload.to_string()))
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(append_res.status(), StatusCode::OK);
		let append_body = to_bytes(append_res.into_body(), usize::MAX).await.unwrap();
		let appended: Value = serde_json::from_slice(&append_body).unwrap();
		assert_eq!(appended["logs"].as_array().unwrap().len(), 1);

		let clear_res = app
			.clone()
			.oneshot(
				Request::builder()
					.method("POST")
					.uri(format!("/api/v1/buckets/{bucket_id}/clear"))
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(clear_res.status(), StatusCode::OK);
		let cleared: Value =
			serde_json::from_slice(&to_bytes(clear_res.into_body(), usize::MAX).await.unwrap())
				.unwrap();
		assert_eq!(cleared["logs"].as_array().unwrap().len(), 0);

		let delete_res = app
			.clone()
			.oneshot(
				Request::builder()
					.method("DELETE")
					.uri(format!("/api/v1/buckets/{bucket_id}"))
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(delete_res.status(), StatusCode::NO_CONTENT);

		let final_list = app
			.oneshot(
				Request::builder()
					.uri("/api/v1/buckets")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		let final_body = to_bytes(final_list.into_body(), usize::MAX).await.unwrap();
		let after: Value = serde_json::from_slice(&final_body).unwrap();
		assert!(after.as_array().unwrap().is_empty());
	}

	#[tokio::test]
	async fn static_js_sets_etag_and_304() {
		let app = Router::new().route("/puppylog.js", get(js));

		let res1 = app
			.clone()
			.oneshot(
				Request::builder()
					.uri("/puppylog.js")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res1.status(), StatusCode::OK);
		let etag = res1
			.headers()
			.get(axum::http::header::ETAG)
			.expect("ETag header present")
			.to_str()
			.unwrap()
			.to_string();

		let res2 = app
			.oneshot(
				Request::builder()
					.uri("/puppylog.js")
					.header(axum::http::header::IF_NONE_MATCH, etag)
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res2.status(), StatusCode::NOT_MODIFIED);
	}

	#[tokio::test]
	async fn static_js_handles_complex_if_none_match_headers() {
		let app = Router::new().route("/puppylog.js", get(js));

		let res1 = app
			.clone()
			.oneshot(
				Request::builder()
					.uri("/puppylog.js")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res1.status(), StatusCode::OK);
		let etag = res1
			.headers()
			.get(axum::http::header::ETAG)
			.expect("ETag header present")
			.to_str()
			.unwrap()
			.to_string();

		let res_with_multiple = app
			.clone()
			.oneshot(
				Request::builder()
					.uri("/puppylog.js")
					.header(
						axum::http::header::IF_NONE_MATCH,
						format!("{}, \"other\"", etag),
					)
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res_with_multiple.status(), StatusCode::NOT_MODIFIED);

		let res_with_weak = app
			.clone()
			.oneshot(
				Request::builder()
					.uri("/puppylog.js")
					.header(axum::http::header::IF_NONE_MATCH, format!("W/{}", etag))
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res_with_weak.status(), StatusCode::NOT_MODIFIED);

		let res_with_star = app
			.oneshot(
				Request::builder()
					.uri("/puppylog.js")
					.header(axum::http::header::IF_NONE_MATCH, "*")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res_with_star.status(), StatusCode::NOT_MODIFIED);
	}

	#[tokio::test]
	async fn server_info_reports_upload_counts() {
		let dir = TempDir::new().unwrap();
		// Point uploads path to temp dir
		std::env::set_var("UPLOAD_PATH", dir.path());

		// Create a couple of files
		let f1 = dir.path().join("a.ready");
		let f2 = dir.path().join("b.part");
		std::fs::write(&f1, b"hello").unwrap();
		std::fs::write(&f2, b"world!").unwrap();

		let app = Router::new().route("/api/v1/server_info", get(get_server_info));
		let res = app
			.oneshot(
				Request::builder()
					.uri("/api/v1/server_info")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();
		assert_eq!(res.status(), StatusCode::OK);
		let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
		let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
		assert!(v.get("freeBytes").is_some());
		assert!(v.get("totalBytes").is_some());
		assert_eq!(v.get("uploadFilesCount").unwrap().as_u64().unwrap(), 2);
		assert_eq!(v.get("uploadBytes").unwrap().as_u64().unwrap(), 5 + 6);
	}
}
