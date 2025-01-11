use std::{collections::HashMap, fs::read_dir, io::{Read, Write}, sync::Arc};

use axum::{
    body::{Body, BodyDataStream}, extract::{DefaultBodyLimit, Path, Query, State}, http::StatusCode, response::{sse::{Event, KeepAlive}, Sse}, routing::{get, post}, Json, Router
};
use chrono::{DateTime, Datelike, Utc};
use futures::Stream;
use futures_util::StreamExt;
use puppylog::{LogEntry, LogEntryParser, LogLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string, Value};
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
	pub props: Vec<(String, String)>,
	pub search: Option<String>,
}

fn log_path() -> std::path::PathBuf {
    match std::env::var("LOG_PATH") {
        Ok(val) => std::path::Path::new(&val).to_owned(),
        Err(_) => std::path::Path::new("./logs").to_owned()
    }
}

fn get_years() -> Vec<u32> {
    let logs_path = log_path();
    let mut years = read_dir(logs_path).unwrap();
    let mut years_vec = Vec::new();
    loop {
        let year = match years.next() {
            Some(year) => year.unwrap(),
            None => break
        };

        let year = year.file_name().into_string().unwrap().parse::<u32>().unwrap();
        years_vec.push(year);
    }

    years_vec
}

fn get_monts(year: u32) -> Vec<u32> {
    let logs_path = log_path();
    let mut months = read_dir(logs_path.join(year.to_string())).unwrap();
    let mut months_vec = Vec::new();
    loop {
        let month = match months.next() {
            Some(month) => month.unwrap(),
            None => break
        };

        let month = month.file_name().into_string().unwrap().parse::<u32>().unwrap();
        months_vec.push(month);
    }

    months_vec
}

fn get_days(year: u32, month: u32) -> Vec<u32> {
    let logs_path = log_path();
    let mut days = read_dir(logs_path.join(year.to_string()).join(month.to_string())).unwrap();
    let mut days_vec = Vec::new();
    loop {
        let day = match days.next() {
            Some(day) => day.unwrap(),
            None => break
        };
        println!("{:?}", day);

        let day = match day.file_name().into_string().unwrap().parse::<u32>() {
            Ok(day) => day,
            Err(_) => continue
        };
        days_vec.push(day);
    }

    days_vec
}


#[tokio::main]
async fn main() {
    // initialize tracing
    //tracing_subscriber::fmt::init();

    let ctx = Context::new();
	let ctx = Arc::new(ctx);

    let cors = CorsLayer::new()
        .allow_origin(Any) // Allow requests from any origin
        .allow_methods(AllowMethods::any()) // Allowed HTTP methods
        .allow_headers(Any);

    // build our application with a route
    let app = Router::new()
        .route("/", get(root))
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
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn upload_logs(State(ctx): State<Arc<Context>>, body: Body) {
    let mut stream: BodyDataStream = body.into_data_stream();
    let mut buffer = Vec::new();
    let mut unprocessed_start = 0;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Append new chunk to buffer
                buffer.extend_from_slice(&chunk);

                // Process as many complete log entries as possible
                let mut read_cursor = 0;
                while let Ok(entry) = LogEntry::deserialize(&mut std::io::Cursor::new(&buffer[unprocessed_start..])) {
                    // Track how much data we read
                    read_cursor += std::io::Cursor::new(&buffer[unprocessed_start + read_cursor..])
                        .position() as usize;
                    
                    // Publish the entry
                    if let Err(e) = ctx.publisher.send(entry).await {
                        eprintln!("Failed to publish log entry: {}", e);
                        return;
                    }
                }
                
                // Update the unprocessed data pointer
                unprocessed_start += read_cursor;

                // If we've processed a significant portion of the buffer, compact it
                if unprocessed_start > buffer.len() / 2 {
                    // Copy remaining unprocessed data to start of buffer
                    buffer.copy_within(unprocessed_start.., 0);
                    buffer.truncate(buffer.len() - unprocessed_start);
                    unprocessed_start = 0;
                }
            }
            Err(e) => {
                eprintln!("Error receiving chunk: {}", e);
                return;
            }
        }
    }

    // Process any remaining complete entries in the buffer
    if unprocessed_start < buffer.len() {
        let mut cursor = std::io::Cursor::new(&buffer[unprocessed_start..]);
        while let Ok(entry) = LogEntry::deserialize(&mut cursor) {
            if let Err(e) = ctx.publisher.send(entry).await {
                eprintln!("Failed to publish final log entry: {}", e);
                return;
            }
        }
    }
}

async fn get_logs(
	State(ctx): State<Arc<Context>>, 
	Query(params): Query<GetLogsQuery>
) -> Json<Value> {
	todo!()
    // let logs_path = log_path();
    // let mut years = get_years();

    // let mut loglines = Vec::new();

    // // if let Some(sort) = &params.sort {
    // //     match sort {
    // //         SortDir::Asc => years.sort(),
    // //         SortDir::Desc => years.sort_by(|a, b| b.cmp(a))
    // //     }
    // // }

    // if let Some(start) = params.start {
    //     years.retain(|year| year >= &(start.year() as u32));
    // }

    // if let Some(end) = params.end {
    //     years.retain(|year| year <= &(end.year() as u32));
    // }

    // 'year_loop: for year in years {
    //     let mut months = get_monts(year);

    //     if let Some(sort) = &params.sort {
    //         match sort {
    //             SortDir::Asc => months.sort(),
    //             SortDir::Desc => months.sort_by(|a, b| b.cmp(a))
    //         }
    //     }

    //     if let Some(start) = params.start {
    //         months.retain(|month| month >= &(start.month() as u32));
    //     }

    //     if let Some(end) = params.end {
    //         months.retain(|month| month <= &(end.month() as u32));
    //     }

    //     for month in months {
    //         let mut days = get_days(year, month);

    //         if let Some(sort) = &params.sort {
    //             match sort {
    //                 SortDir::Asc => days.sort(),
    //                 SortDir::Desc => days.sort_by(|a, b| b.cmp(a))
    //             }
    //         }

    //         if let Some(start) = params.start {
    //             days.retain(|day| day >= &(start.day() as u32));
    //         }

    //         if let Some(end) = params.end {
    //             days.retain(|day| day <= &(end.day() as u32));
    //         }

    //         for day in days {
    //             let files = read_dir(logs_path.join(year.to_string()).join(month.to_string()).join(day.to_string())).unwrap();
    //             for file in files {
    //                 //let devid = file.unwrap().file_name().into_string().unwrap().replace(".log", "");
                    
    //                 let mut file = std::fs::File::open(file.unwrap().path()).unwrap();
    //                 let mut contents = String::new();
    //                 file.read_to_string(&mut contents).unwrap();
    //                 for line in contents.lines() {
    //                     let logline = logline::parse_logline(line);
    //                     loglines.push(logline);

    //                     if let Some(limit) = params.count {
    //                         if loglines.len() >= limit as usize {
    //                             break 'year_loop;
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }
    // }

    // Json(serde_json::to_value(loglines).unwrap())
}

async fn stream_logs(
    State(ctx): State<Arc<Context>>,
    Query(params): Query<GetLogsQuery>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    println!("stream logs {:?}", params);
    
    let rx = ctx.subscriber.subscribe(LogsQuery {
        start: params.start,
        end: params.end,
        level: params.level,
        search: params.search,
        props: params.props
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|p| {
            let data = to_string(&p).unwrap();
            Ok(Event::default().data(data))
        });
    Sse::new(stream)
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