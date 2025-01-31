use std::sync::Arc;
use axum::body::Body;
use axum::body::BodyDataStream;
use axum::extract::DefaultBodyLimit;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::Event;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::response::Sse;
use axum::routing::get;
use axum::routing::post;
use axum::Json;
use axum::Router;
use futures::Stream;
use futures_util::StreamExt;
use log::LevelFilter;
use log_query::parse_log_query;
use log_query::QueryAst;
use puppylog::LogEntry;
use puppylog::LogEntryChunkParser;
use serde::Deserialize;
use serde_json::json;
use serde_json::to_string;
use serde_json::Value;
use simple_logger::SimpleLogger;
use storage::search_logs;
use storage::Storage;
use tokio::io::AsyncReadExt;
use tower_http::cors::AllowMethods;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::decompression::RequestDecompressionLayer;
use types::Context;

mod logline;
mod cache;
mod storage;
mod picker;
mod types;
mod worker;
mod subscriber;
mod config;
mod log_query;
mod query_eval;

#[derive(Deserialize, Debug)]
enum SortDir {
	Asc,
	Desc
}

#[derive(Deserialize, Debug)]
struct GetLogsQuery {
	pub offset: Option<usize>,
	pub count: Option<usize>,
	pub query: Option<String>,
}


#[tokio::main]
async fn main() {
	// initialize tracing
	//tracing_subscriber::fmt::init();
	SimpleLogger::new().with_level(LevelFilter::Info).init().unwrap();

	let ctx = Context::new();
	let ctx = Arc::new(ctx);

	let cors = CorsLayer::new()
		.allow_origin(Any) // Allow requests from any origin
		.allow_methods(AllowMethods::any()) // Allowed HTTP methods
		.allow_headers(Any);

	// build our application with a route
	let app = Router::new()
		.route("/", get(root))
		.route("/puppylog.js", get(js))
		.route("/favicon.ico", get(favicon))
		.route("/favicon-192x192.png", get(favicon_192x192))
		.route("/favicon-512x512.png", get(favicon_512x512))
		.route("/manifest.json", get(manifest))
		.route("/api/logs", get(get_logs)).layer(cors.clone())
		.route("/api/logs/stream", get(stream_logs)).layer(cors)
		.route("/api/logs", post(upload_logs))
			.layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
			.layer(RequestDecompressionLayer::new().gzip(true).zstd(true))
			.with_state(ctx)
		.fallback(get(root));

	// run our app with hyper, listening globally on port 3000
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

const INDEX_HTML: &str = include_str!("../assets/index.html");
const JS_HTML: &str = include_str!("../assets/puppylog.js");
const FAVICON: &[u8] = include_bytes!("../assets/favicon.ico");
const FAVICON_192x192: &[u8] = include_bytes!("../assets/favicon-192x192.png");
const FAVICON_512x512: &[u8] = include_bytes!("../assets/favicon-512x512.png");

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

async fn upload_logs(State(ctx): State<Arc<Context>>, body: Body) {
	let mut stream: BodyDataStream = body.into_data_stream();
	let mut storage = Storage::new();
	let mut i = 0;
	let mut chunk_reader = LogEntryChunkParser::new();
	
	while let Some(chunk_result) = stream.next().await {
		match chunk_result {
			Ok(chunk) => {
				log::info!("Received chunk of size {}", chunk.len());
				chunk_reader.add_chunk(chunk);
				for entry in &chunk_reader.log_entries {
					log::info!("[{}] parsed", i);
					i += 1;
					if let Err(err) = storage.save_log_entry(&entry).await {
						log::error!("Failed to save log entry: {}", err);
						return;
					}
					if let Err(e) = ctx.publisher.send(entry.clone()).await {
						log::error!("Failed to publish log entry: {}", e);
					}
				}
				chunk_reader.log_entries.clear();
			}
			Err(e) => {
				log::error!("Error receiving chunk: {}", e);
				return;
			}
		}
	}
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
		Some(ref query) => match parse_log_query(query) {
			Ok(query) => query,
			Err(err) => return Err(BadRequestError(err.to_string()))
		}
		None => QueryAst::default()
	};

	log::info!("query: {:?}", query);
	query.offset = params.offset;
	query.limit = params.count;

	let log_entries = search_logs(query).await.unwrap();
	let log_entries = log_entries.into_iter().map(|entry| logentry_to_json(&entry)).collect::<Vec<_>>();
	Ok(Json(serde_json::to_value(&log_entries).unwrap()))
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