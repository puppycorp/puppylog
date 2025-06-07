use axum::body::Body;
use axum::body::BodyDataStream;
use axum::extract::DefaultBodyLimit;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::header;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::sse::Event;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::response::Sse;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use axum::Json;
use axum::Router;
use chrono::DateTime;
use chrono::Utc;
use config::log_path;
use config::upload_path;
use context::Context;
use db::MetaProp;
use db::UpdateDeviceSettings;
use db::UpdateDevicesSettings;
use futures::executor::block_on;
use futures::Stream;
use futures::StreamExt;
use log::LevelFilter;
use puppylog::*;
use rand::Rng;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::to_string;
use serde_json::Value;
use simple_logger::SimpleLogger;
use std::sync::Arc;
use tokio::fs;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;
use tower_http::compression::CompressionLayer;
use tower_http::cors::AllowMethods;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::decompression::RequestDecompressionLayer;
use types::GetSegmentsQuery;

mod background;
mod cache;
mod config;
mod context;
mod db;
mod logline;
mod segment;
mod settings;
mod slack;
mod subscribe_worker;
mod types;
mod upload_guard;
mod utility;
mod wal;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GetLogsQuery {
	pub count: Option<usize>,
	pub query: Option<String>,
	pub end_date: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GetHistogramQuery {
	pub query: Option<String>,
	pub bucket_secs: Option<u64>,
	pub end_date: Option<DateTime<Utc>>,
}

#[tokio::main]
async fn main() {
	SimpleLogger::new()
		.with_level(LevelFilter::Info)
		.init()
		.unwrap();
	let log_path = log_path();
	log::info!("checking if log path exists: {:?}", log_path);
	if !log_path.exists() {
		log::info!("does not exist, creating it");
		std::fs::create_dir_all(log_path).unwrap();
	}
	let ctx = Context::new().await;
	let ctx = Arc::new(ctx);

	// Spawn background worker that ingests staged log uploads
	tokio::spawn(background::process_log_uploads(ctx.clone()));

	let cors = CorsLayer::new()
		.allow_origin(Any) // Allow requests from any origin
		.allow_methods(AllowMethods::any()) // Allowed HTTP methods
		.allow_headers(Any);

	// build our application with a route
	let app = Router::new()
		.route("/", get(root))
		.route("/puppylog.js", get(js))
		.route("/puppylog.css", get(css))
		.route("/favicon.ico", get(favicon))
		.route("/favicon-192x192.png", get(favicon_192x192))
		.route("/favicon-512x512.png", get(favicon_512x512))
		.route("/manifest.json", get(manifest))
		.route("/api/logs", get(get_logs))
		.layer(CompressionLayer::new())
		.layer(cors.clone())
		.route("/api/logs/stream", get(stream_logs))
		.layer(cors.clone())
		.route("/api/settings/query", post(post_settings_query))
		.with_state(ctx.clone())
		.route("/api/settings/query", get(get_settings_query))
		.with_state(ctx.clone())
		.route("/api/segments", get(get_segments))
		.with_state(ctx.clone())
		.route("/api/segment/metadata", get(get_segment_metadata))
		.with_state(ctx.clone())
		.route("/api/v1/validate_query", get(validate_query))
		.route("/api/v1/logs/stream", get(stream_logs))
		.layer(cors.clone())
		.route("/api/v1/logs/histogram", get(get_histogram))
		.layer(cors.clone())
		.route("/api/v1/device/settings", post(update_devices_settings))
		.with_state(ctx.clone())
		.route("/api/v1/device/{deviceId}/status", get(get_device_status))
		.layer(cors.clone())
		.with_state(ctx.clone())
		.route("/api/v1/device/{deviceId}/logs", post(upload_device_logs))
		.layer(cors.clone())
		.layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
		.layer(RequestDecompressionLayer::new().gzip(true).zstd(true))
		.with_state(ctx.clone())
		.route(
			"/api/v1/device/{deviceId}/metadata",
			post(update_device_metadata),
		)
		.with_state(ctx.clone())
		.route(
			"/api/v1/device/{deviceId}/settings",
			post(update_device_settings),
		)
		.with_state(ctx.clone())
		.route("/api/v1/device/bulkedit", post(bulk_edit))
		.with_state(ctx.clone())
		.route("/api/v1/settings", post(post_settings_query))
		.with_state(ctx.clone())
		.route("/api/v1/settings", get(get_settings_query))
		.with_state(ctx.clone())
		.route("/api/v1/devices", get(get_devices))
		.with_state(ctx.clone())
		.route("/api/v1/segments", get(get_segments))
		.with_state(ctx.clone())
		.route("/api/v1/segment/{segmentId}", get(get_segment))
		.with_state(ctx.clone())
		.route("/api/v1/segment/{segmentId}/props", get(get_segment_props))
		.with_state(ctx.clone())
		.route(
			"/api/v1/segment/{segmentId}/download",
			get(download_segment),
		)
		.route("/api/v1/segment/{segmentId}", delete(delete_segment))
		.with_state(ctx.clone())
		.fallback(get(root));

	// run our app with hyper, listening globally on port 3000
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

async fn get_segment_metadata(State(ctx): State<Arc<Context>>) -> Json<Value> {
	let segments = ctx.db.fetch_segments_metadata().await.unwrap();
	Json(serde_json::to_value(&segments).unwrap())
}

async fn get_segment(State(ctx): State<Arc<Context>>, Path(segment_id): Path<u32>) -> Json<Value> {
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

async fn delete_segment(
	State(ctx): State<Arc<Context>>,
	Path(segment_id): Path<u32>,
) -> &'static str {
	log::info!("delete_segment: {:?}", segment_id);
	ctx.db.delete_segment(segment_id).await.unwrap();
	fs::remove_file(log_path().join(format!("{}.log", segment_id)))
		.await
		.unwrap();
	"ok"
}

async fn get_segment_props(
	State(ctx): State<Arc<Context>>,
	Path(segment_id): Path<u32>,
) -> Json<Value> {
	let props = ctx.db.fetch_segment_props(segment_id).await.unwrap();
	Json(serde_json::to_value(&props).unwrap())
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BulkEdit {
	pub filter_level: LogLevel,
	pub send_logs: bool,
	pub send_interval: u32,
	pub device_ids: Vec<String>,
}

async fn bulk_edit(State(ctx): State<Arc<Context>>, body: Json<BulkEdit>) -> &'static str {
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

async fn validate_query(Query(params): Query<GetLogsQuery>) -> Result<(), BadRequestError> {
	log::info!("validate_query {:?}", params);
	match params.query {
		Some(ref q) => {
			let q = q.replace("\n", "");
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

async fn update_device_settings(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
	body: Json<UpdateDeviceSettings>,
) -> &'static str {
	log::info!("update_device_settings device_id: {:?}", device_id);
	ctx.db.update_device_settings(&device_id, &body).await;
	"ok"
}

async fn get_devices(State(ctx): State<Arc<Context>>) -> Json<Value> {
	let devices = ctx.db.get_devices().await.unwrap();
	Json(serde_json::to_value(&devices).unwrap())
}

async fn get_segments(
	State(ctx): State<Arc<Context>>,
	Query(mut params): Query<GetSegmentsQuery>,
) -> Json<Value> {
	if params.count.is_none() {
		params.count = Some(100);
	}
	let segments = ctx.db.find_segments(&params).await.unwrap();
	Json(serde_json::to_value(&segments).unwrap())
}

async fn upload_device_logs(
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

async fn update_devices_settings(
	State(ctx): State<Arc<Context>>,
	body: Json<UpdateDevicesSettings>,
) -> &'static str {
	log::info!("update_devices_settings: {:?}", body);
	ctx.db.update_devices_settings(&body).await;
	"ok"
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

async fn get_device_status(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
) -> Json<Value> {
	let device = ctx.db.get_or_create_device(&device_id).await.unwrap();

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

async fn update_device_metadata(
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

#[cfg(debug_assertions)]
async fn css() -> String {
	let mut file = tokio::fs::File::open("assets/puppylog.css").await.unwrap();
	let mut contents = String::new();
	file.read_to_string(&mut contents).await.unwrap();
	contents
}

#[cfg(not(debug_assertions))]
async fn css() -> &'static str {
	CSS
}

// basic handler that responds with a static string
async fn root() -> Html<&'static str> {
	Html(INDEX_HTML)
}

#[cfg(debug_assertions)]
async fn js() -> String {
	let mut file = tokio::fs::File::open("assets/puppylog.js").await.unwrap();
	let mut contents = String::new();
	file.read_to_string(&mut contents).await.unwrap();
	contents
}

#[cfg(not(debug_assertions))]
async fn js() -> &'static str {
	JS_HTML
}

#[derive(Deserialize, Debug)]
struct UpdateQuery {
	pub query: String,
}

async fn post_settings_query(State(ctx): State<Arc<Context>>, body: String) -> &'static str {
	log::info!("post_settings_query: {:?}", body);
	let mut settings = ctx.settings.inner().await;
	settings.collection_query = body.clone();
	settings.save().unwrap();
	ctx.event_tx
		.send(PuppylogEvent::QueryChanged { query: body })
		.unwrap();
	"ok"
}

async fn get_settings_query(State(ctx): State<Arc<Context>>) -> String {
	let settings = ctx.settings.inner().await;
	settings.collection_query.clone()
}

async fn favicon() -> Result<Response, StatusCode> {
	Ok((
		StatusCode::OK,
		[(axum::http::header::CONTENT_TYPE, "image/x-icon")],
		FAVICON,
	)
		.into_response())
}

async fn favicon_192x192() -> Result<Response, StatusCode> {
	Ok((
		StatusCode::OK,
		[(axum::http::header::CONTENT_TYPE, "image/png")],
		FAVICON_192x192,
	)
		.into_response())
}

async fn favicon_512x512() -> Result<Response, StatusCode> {
	Ok((
		StatusCode::OK,
		[(axum::http::header::CONTENT_TYPE, "image/png")],
		FAVICON_512x512,
	)
		.into_response())
}

async fn manifest() -> Result<Response, StatusCode> {
	Ok((
		StatusCode::OK,
		[(axum::http::header::CONTENT_TYPE, "application/json")],
		include_bytes!("../assets/manifest.json"),
	)
		.into_response())
}

#[derive(Debug)]
struct BadRequestError(String);

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

async fn get_logs(
	State(ctx): State<Arc<Context>>,
	Query(params): Query<GetLogsQuery>,
	headers: HeaderMap,
) -> Result<Response, BadRequestError> {
	log::info!("get_logs {:?}", params);
	let mut query = match params.query {
		Some(ref q) => {
			let q = q.replace("\n", "");
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
	query.limit = params.count;
	query.end_date = match params.end_date {
		Some(end_date) => Some(end_date),
		None => Some(Utc::now() + chrono::Duration::days(200)),
	};

	let (tx, rx) = mpsc::channel(100);
	let producer_query = query.clone();
	let ctx_clone = Arc::clone(&ctx);
	spawn_blocking(move || {
		let end = producer_query
			.end_date
			.unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::days(200));
		let count = producer_query.limit.unwrap_or(200);
		let mut sent = 0;
		block_on(ctx_clone.find_logs(query, |entry| {
			if tx.is_closed() {
				return false;
			}
			let log_json = logentry_to_json(entry);
			if tx.blocking_send(log_json).is_err() {
				return false;
			}
			sent += 1;
			if sent >= count {
				return false;
			}
			true
		}));
	});

	let wants_stream = headers
		.get("accept")
		.and_then(|v| v.to_str().ok())
		.map(|s| s.contains("text/event-stream"))
		.unwrap_or(false);

	let res = if wants_stream {
		let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|log| {
			let data = to_string(&log).unwrap();
			Ok::<Event, std::convert::Infallible>(Event::default().data(data))
		});
		Sse::new(stream).into_response()
	} else {
		let logs: Vec<_> = ReceiverStream::new(rx).collect::<Vec<_>>().await;
		Json(serde_json::to_value(&logs).unwrap()).into_response()
	};
	Ok(res)
}

async fn stream_logs(
	State(ctx): State<Arc<Context>>,
	Query(params): Query<GetLogsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, BadRequestError> {
	log::info!("stream logs {:?}", params);
	let query = match params.query {
		Some(ref query) => match parse_log_query(query) {
			Ok(query) => query,
			Err(err) => return Err(BadRequestError(err.to_string())),
		},
		None => QueryAst::default(),
	};
	let rx = ctx.subscriber.subscribe(query).await;
	let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|p| {
		let data = to_string(&logentry_to_json(&p)).unwrap();
		Ok(Event::default().data(data))
	});
	Ok(Sse::new(stream))
}

async fn get_histogram(
	State(ctx): State<Arc<Context>>,
	Query(params): Query<GetHistogramQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, BadRequestError> {
	log::info!("get histogram {:?}", params);
	let bucket_secs = params.bucket_secs.unwrap_or(60);
	let query = match params.query {
		Some(ref q) => match parse_log_query(q) {
			Ok(q) => q,
			Err(err) => return Err(BadRequestError(err.to_string())),
		},
		None => QueryAst::default(),
	};
	let (tx, rx) = mpsc::channel(100);
	let ctx_clone = Arc::clone(&ctx);
	let producer_query = query.clone();
	spawn_blocking(move || {
		let mut current_bucket: Option<i64> = None;
		let mut count: u64 = 0;
		block_on(ctx_clone.find_logs(producer_query, |entry| {
			let ts = entry.timestamp.timestamp();
			let bucket = ts - ts % bucket_secs as i64;
			if let Some(cb) = current_bucket {
				if bucket != cb {
					if tx.is_closed() {
						return false;
					}
					let item = json!({
					"timestamp": DateTime::<Utc>::from_timestamp(cb, 0).unwrap(),
					"count": count,
					});
					if tx.blocking_send(item).is_err() {
						return false;
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
			true
		}));
		if let Some(cb) = current_bucket {
			let item = json!({
			"timestamp": DateTime::<Utc>::from_timestamp(cb, 0).unwrap(),
			"count": count,
			});
			let _ = tx.blocking_send(item);
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
	use axum::body::Body;
	use axum::http::{Request, StatusCode};
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

		let ctx = Arc::new(Context::new().await);

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
}
