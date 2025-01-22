use std::{collections::{HashMap, VecDeque}, fs::read_dir, io::{Read, Write}, process::Child, sync::Arc};

use axum::{
    body::{Body, BodyDataStream}, extract::{DefaultBodyLimit, Path, Query, State}, http::StatusCode, response::{sse::{Event, KeepAlive}, Html, IntoResponse, Response, Sse}, routing::{get, post}, Json, Router
};
use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use config::log_path;
use futures::Stream;
use futures_util::StreamExt;
use log::LevelFilter;
use log_query::{parse_log_query, QueryAst};
use puppylog::{ChunckReader, CircularBuffer, LogEntry, LogLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string, Value};
use simple_logger::SimpleLogger;
use storage::{search_logs, Storage};
use tokio::{fs, io::AsyncReadExt, sync::mpsc};
use tower_http::{cors::{AllowMethods, Any, CorsLayer}, decompression::{DecompressionLayer, RequestDecompressionLayer}};
use types::{Context, LogsQuery, SubscribeReq};

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
	pub start: Option<DateTime<Utc>>,
	pub end: Option<DateTime<Utc>>,
	pub level: Option<LogLevel>,
    pub offset: Option<usize>,
    pub count: Option<usize>,
	pub props: Option<Vec<(String, String)>>,
	pub search: Option<String>,
    pub query: Option<String>,
    pub timezone: Option<i32>,
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
        .route("/api/device/{devid}/rawlogs", post(upload_raw_logs))
            .layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
            .layer(RequestDecompressionLayer::new().gzip(true))
        .route("/api/device/{devid}/rawlogs/stream", post(stream_raw_logs))
            .layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
            .layer(RequestDecompressionLayer::new().gzip(true))
        .route("/api/logs", get(get_logs)).layer(cors.clone())
        .route("/api/logs/stream", get(stream_logs)).layer(cors)
		.route("/api/logs", post(upload_logs))
			.layer(RequestDecompressionLayer::new().gzip(true))
			.with_state(ctx);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3337").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

const INDEX_HTML: &str = include_str!("../assets/index.html");
const JS_HTML: &str = include_str!("../assets/puppylog.js");

// basic handler that responds with a static string
async fn root() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn js() -> &'static str {
    JS_HTML
}

async fn upload_logs(State(ctx): State<Arc<Context>>, body: Body) {
    let mut stream: BodyDataStream = body.into_data_stream();
    let mut storage = Storage::new();
    let mut i = 0;
	//let mut buffer = CircularBuffer::new(30000);
	let mut chunk_reader = ChunckReader::new();
	
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
				log::info!("Received chunk of size {}", chunk.len());
				chunk_reader.add_chunk(chunk);
				loop {
					match LogEntry::deserialize(&mut chunk_reader) {
						Ok(entry) => {
							chunk_reader.commit();
							log::info!("[{}] parsed", i);
							i += 1;
							if let Err(err) = storage.save_log_entry(&entry).await {
								log::error!("Failed to save log entry: {}", err);
								return;
							}
							if let Err(e) = ctx.publisher.send(entry).await {
								log::error!("Failed to publish log entry: {}", e);
							}
						},
						Err(err) => {
							chunk_reader.rollback();
							log::error!("{}", err);
							break;
						}
					}
				}
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
    // log::info!("log_entries: {:?}", log_entries);
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
			let data = to_string(&p).unwrap();
			Ok(Event::default().data(data))
		});

	Ok(Sse::new(stream))
}

async fn upload_raw_logs(
    Path(devid): Path<String>,
    body: String,
) {
    //println!("{}", body);

    let now = Utc::now();

    println!("logpath: {}", log_path().display());

    let path = log_path().join(format!("{}/{}/{}", now.year(), now.month(), now.day()));

    println!("{}", path.display());

    if !path.exists() {
        std::fs::create_dir_all(&path).unwrap();
    }

    let file = path.join(format!("{}.log", devid));

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .unwrap();

    file.write_all(body.as_bytes()).unwrap();

    println!("writing done");
}

async fn stream_raw_logs(Path(devid): Path<String>, body: Body)  {
    println!("stream_raw_logs");
    let now = Utc::now();

    println!("logpath: {}", log_path().display());

    let path = log_path().join(format!("{}/{}/{}", now.year(), now.month(), now.day()));

    println!("{}", path.display());

    if !path.exists() {
        std::fs::create_dir_all(&path).unwrap();
    }

    let file = path.join(format!("{}.log", devid));

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .unwrap();

    let mut stream: BodyDataStream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        file.write(&chunk).unwrap();
    }
}