use std::sync::Arc;
use axum::body::Body;
use axum::body::BodyDataStream;
use axum::extract::ws::WebSocket;
use axum::extract::DefaultBodyLimit;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::http::StatusCode;
use axum::response::sse::Event;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::response::Sse;
use axum::routing::any;
use axum::routing::get;
use axum::routing::post;
use axum::Json;
use axum::Router;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Utc;
use config::log_path;
use futures::Stream;
use futures_util::StreamExt;
use log::LevelFilter;
use puppylog::*;
use serde::Deserialize;
use serde_json::json;
use serde_json::to_string;
use serde_json::Value;
use simple_logger::SimpleLogger;
use tokio::io::AsyncReadExt;
use tokio::time::Instant;
use tower_http::compression::CompressionLayer;
use tower_http::cors::AllowMethods;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::decompression::RequestDecompressionLayer;
use context::Context;
use context::DeviceStatus;

mod logline;
mod cache;
mod storage;
mod context;
mod worker;
mod subscriber;
mod config;
mod settings;
mod db;
mod segment;
mod wal;

#[derive(Deserialize, Debug)]
enum SortDir {
	Asc,
	Desc
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GetLogsQuery {
	pub count: Option<usize>,
	pub query: Option<String>,
	pub end_date: Option<DateTime<Utc>>,
}


#[tokio::main]
async fn main() {
	SimpleLogger::new().with_level(LevelFilter::Info).init().unwrap();
	let log_path = log_path();
	log::info!("checking if log path exists: {:?}", log_path);
	if !log_path.exists() {
		log::info!("does not exist, creating it");
		std::fs::create_dir_all(log_path).unwrap();
	}
	let ctx = Context::new().await;
	let ctx = Arc::new(ctx);

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
		.route("/api/logs", get(get_logs)).layer(CompressionLayer::new()).layer(cors.clone())
		.route("/api/logs/stream", get(stream_logs)).layer(cors.clone())
		.route("/api/settings/query", post(post_settings_query)).with_state(ctx.clone())
		.route("/api/settings/query", get(get_settings_query)).with_state(ctx.clone())
		.route("/api/device/{deviceId}/ws", any(device_ws_handler)).with_state(ctx.clone())
		.route("/api/v1/logs", get(get_logs)).layer(cors.clone())
		.route("/api/v1/logs/stream", get(stream_logs)).layer(cors.clone())
		.route("/api/v1/device/{deviceId}/ws", any(device_ws_handler)).with_state(ctx.clone())
		.route("/api/v1/device/{deviceId}/status", get(get_device_status)).layer(cors.clone())
		.route("/api/v1/device/{deviceId}/logs", post(upload_device_logs))
			.layer(cors.clone())
			.layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
			.layer(RequestDecompressionLayer::new().gzip(true).zstd(true))
			.with_state(ctx.clone())
		.route("/api/v1/settings", post(post_settings_query)).with_state(ctx.clone())
		.route("/api/v1/settings", get(get_settings_query)).with_state(ctx.clone())
		.route("/api/v1/devices", get(get_devices)).with_state(ctx.clone())
		.fallback(get(root));

	// run our app with hyper, listening globally on port 3000
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
	axum::serve(
		listener,
		app,
	).await.unwrap();
}

async fn get_devices(State(ctx): State<Arc<Context>>) -> Json<Value> {
	let devices = ctx.db.get_devices().await.unwrap();
	Json(serde_json::to_value(&devices).unwrap())
}

async fn upload_device_logs(
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>,
	body: Body
) {
	log::info!("upload_device_logs device_id: {}", device_id);
	let upload_timer = Instant::now();
	let mut stream: BodyDataStream = body.into_data_stream();
	let mut chunk_reader = LogEntryChunkParser::new();
	let mut i = 0;
	let mut total_bytes = 0;
	while let Some(chunk_result) = stream.next().await {
		match chunk_result {
			Ok(chunk) => {
				total_bytes += chunk.len();
				chunk_reader.add_chunk(chunk);
				i += chunk_reader.log_entries.len();
			}
			Err(e) => {
				log::error!("Error receiving chunk: {}", e);
				return;
			}
		}
	}
	log::info!("uploaded {} logs in {:?}", i, upload_timer.elapsed());
	let timer = Instant::now();
	if let Err(err) = ctx.db.update_device_metadata(&device_id, total_bytes, chunk_reader.log_entries.len()).await {
		log::error!("Failed to update device metadata: {}", err);
	}
	ctx.save_logs(&chunk_reader.log_entries).await;
	log::info!("saved {} logs in {:?}", i, timer.elapsed());
}

async fn get_device_status(Path(device_id): Path<String>) -> Json<Value> {
	log::info!("get_device_status device_id: {}", device_id);
	let status = DeviceStatus {
		query: None,
		level: None,
		send_logs: true
	};
	Json(serde_json::to_value(&status).unwrap())
}

async fn device_ws_handler(
	ws: WebSocketUpgrade,
	State(ctx): State<Arc<Context>>,
	Path(device_id): Path<String>
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, device_id, ctx))
}

async fn handle_socket(mut socket: WebSocket, device_id: String, ctx: Arc<Context>) {
	log::info!("device connected: {:?}", device_id);
	let mut rx = ctx.event_tx.subscribe();
	let mut chunk_reader = LogEntryChunkParser::new();
	{
		let settings = ctx.settings.inner().await;
		let query = settings.collection_query.clone();
		let event = PuppylogEvent::QueryChanged { query };
		let txt = to_string(&event).unwrap();
		log::info!("Sending event to client {:?}", txt);
		socket.send(axum::extract::ws::Message::Text(txt.into())).await.unwrap();
	}
	loop {
		tokio::select! {
			e = rx.recv() => {
				match e {
					Ok(e) => {
						let txt = to_string(&e).unwrap();
						log::info!("Sending event to client {:?}", txt);
						socket.send(axum::extract::ws::Message::Text(txt.into())).await.unwrap();
					},
					Err(err) => {},
				}
			}
			msg = socket.recv() => {
				match msg {
					Some(Ok(msg)) => {
						// log::info!("Received message: {:?}", msg);
						match msg {
							axum::extract::ws::Message::Text(utf8_bytes) => {},
							axum::extract::ws::Message::Binary(bytes) => {
								let bytes_len = bytes.len();
								log::info!("received batch of {} bytes", bytes_len);
								chunk_reader.add_chunk(bytes);
								let timer = Instant::now();
								log::info!("saving {} logs", chunk_reader.log_entries.len());
								if let Err(err) = ctx.db.update_device_metadata(&device_id, bytes_len, chunk_reader.log_entries.len()).await {
									log::error!("Failed to update device metadata: {}", err);
								}
								ctx.save_logs(&chunk_reader.log_entries).await;
								log::info!("saved {} logs in {:?}", chunk_reader.log_entries.len(), timer.elapsed());
								chunk_reader.log_entries.clear();
							},
							axum::extract::ws::Message::Ping(bytes) => {},
							axum::extract::ws::Message::Pong(bytes) => {},
							axum::extract::ws::Message::Close(close_frame) => {
								log::info!("Connection closed: {:?}", close_frame);
								break;
							},
						}
					}
					Some(Err(err)) => {
						log::error!("Error receiving message: {}", err);
					}
					None => {
						log::error!("Connection closed");
						break;
					}
				}
			}
		}
	}
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
	ctx.event_tx.send(PuppylogEvent::QueryChanged { query: body }).unwrap();
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
	).into_response())
}

async fn favicon_192x192() -> Result<Response, StatusCode> {
	Ok((
		StatusCode::OK,
		[(axum::http::header::CONTENT_TYPE, "image/png")],
		FAVICON_192x192,
	).into_response())
}

async fn favicon_512x512() -> Result<Response, StatusCode> {
	Ok((
		StatusCode::OK,
		[(axum::http::header::CONTENT_TYPE, "image/png")],
		FAVICON_512x512,
	).into_response())
}

async fn manifest() -> Result<Response, StatusCode> {
	Ok((
		StatusCode::OK,
		[(axum::http::header::CONTENT_TYPE, "application/json")],
		include_bytes!("../assets/manifest.json"),
	).into_response())
}

#[derive(Debug)]
struct BadRequestError(String);

impl IntoResponse for BadRequestError {
	fn into_response(self) -> Response {
		(
			StatusCode::BAD_REQUEST,
			Json(json!({
				"error": self.0
			}))
		).into_response()
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
	Query(params): Query<GetLogsQuery>
) -> Result<Json<Value>, BadRequestError> {
	log::info!("get_logs {:?}", params);
	let mut query = match params.query {
		Some(ref query) => {
			let query = query.replace("\n", "");
			let query = query.trim();
			if query.is_empty() {
				log::info!("query is empty");
				QueryAst::default()
			} else {
				log::info!("query: {:?}", query.as_bytes());
				match parse_log_query(&query) {
					Ok(query) => query,
					Err(err) => return Err(BadRequestError(err.to_string()))
				}
			}
		}
		None => QueryAst::default()
	};
	query.limit = params.count;
	query.end_date = params.end_date;
	let timer = Instant::now();
	let end = query.end_date.unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::days(200));
	let count = query.limit.unwrap_or(200);
	let mut logs = Vec::new();
	ctx.find_logs(end, |entry| {
		if check_expr(&query.root, &entry).unwrap() {
			logs.push(logentry_to_json(entry));
		}
		if logs.len() >= count {
			false
		} else {
			true
		}
	}).await;
	Ok(Json(serde_json::to_value(&logs).unwrap()))
}

async fn stream_logs(
	State(ctx): State<Arc<Context>>,
	Query(params): Query<GetLogsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, BadRequestError> {
	log::info!("stream logs {:?}", params);
	let query = match params.query {
		Some(ref query) => match parse_log_query(query) {
			Ok(query) => query,
			Err(err) => return Err(BadRequestError(err.to_string()))
		},
		None => QueryAst::default(),
	};
	let rx = ctx.subscriber.subscribe(query).await;
	let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
		.map(|p| {
			let data = to_string(&logentry_to_json(&p)).unwrap();
			Ok(Event::default().data(data))
		});
	Ok(Sse::new(stream))
}