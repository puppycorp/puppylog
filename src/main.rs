use std::{collections::HashMap, fs::read_dir, io::{Read, Write}};

use axum::{
    body::{Body, BodyDataStream}, extract::{DefaultBodyLimit, Path, Query}, http::StatusCode, response::{sse::Event, Sse}, routing::{get, post}, Json, Router
};
use chrono::{DateTime, Datelike, Utc};
use futures::Stream;
use futures_util::StreamExt;
use logline::LogEntryParser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{fs, io::AsyncReadExt, sync::mpsc};
use tower_http::{cors::{AllowMethods, Any, CorsLayer}, decompression::{DecompressionLayer, RequestDecompressionLayer}};

mod logline;



#[derive(Deserialize, Debug)]
enum SortDir {
    Asc,
    Desc
}

#[derive(Deserialize, Debug)]
struct GetLogsQuery {
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    sort: Option<SortDir>,
    loglevel: Option<String>,
    project: Option<String>,
    env: Option<String>,
    device: Option<String>,
    search: Option<String>,
    count: Option<u32>
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
		.route("/api/logs", post(upload_logs));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn upload_logs(body: Body) {
	let mut stream: BodyDataStream = body.into_data_stream();
	let mut logentry_parser = LogEntryParser::new();

	for chunk in stream.next().await {
		let chunk = chunk.unwrap();
		logentry_parser.parse(&chunk, |entry| {
			println!("{:?}", entry);
		});
	}
}

async fn get_logs(Query(params): Query<GetLogsQuery>) -> Json<Value> {
    let logs_path = log_path();
    let mut years = get_years();

    let mut loglines = Vec::new();

    if let Some(sort) = &params.sort {
        match sort {
            SortDir::Asc => years.sort(),
            SortDir::Desc => years.sort_by(|a, b| b.cmp(a))
        }
    }

    if let Some(start) = params.start {
        years.retain(|year| year >= &(start.year() as u32));
    }

    if let Some(end) = params.end {
        years.retain(|year| year <= &(end.year() as u32));
    }

    'year_loop: for year in years {
        let mut months = get_monts(year);

        if let Some(sort) = &params.sort {
            match sort {
                SortDir::Asc => months.sort(),
                SortDir::Desc => months.sort_by(|a, b| b.cmp(a))
            }
        }

        if let Some(start) = params.start {
            months.retain(|month| month >= &(start.month() as u32));
        }

        if let Some(end) = params.end {
            months.retain(|month| month <= &(end.month() as u32));
        }

        for month in months {
            let mut days = get_days(year, month);

            if let Some(sort) = &params.sort {
                match sort {
                    SortDir::Asc => days.sort(),
                    SortDir::Desc => days.sort_by(|a, b| b.cmp(a))
                }
            }

            if let Some(start) = params.start {
                days.retain(|day| day >= &(start.day() as u32));
            }

            if let Some(end) = params.end {
                days.retain(|day| day <= &(end.day() as u32));
            }

            for day in days {
                let files = read_dir(logs_path.join(year.to_string()).join(month.to_string()).join(day.to_string())).unwrap();
                for file in files {
                    //let devid = file.unwrap().file_name().into_string().unwrap().replace(".log", "");
                    
                    let mut file = std::fs::File::open(file.unwrap().path()).unwrap();
                    let mut contents = String::new();
                    file.read_to_string(&mut contents).unwrap();
                    for line in contents.lines() {
                        let logline = logline::parse_logline(line);
                        loglines.push(logline);

                        if let Some(limit) = params.count {
                            if loglines.len() >= limit as usize {
                                break 'year_loop;
                            }
                        }
                    }
                }
            }
        }
    }

    Json(serde_json::to_value(loglines).unwrap())
}

async fn stream_logs(Query(params): Query<GetLogsQuery>) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    println!("stream logs {:?}", params);
    let logs_path = log_path();
    let mut years = get_years();

    // Apply sorting and filtering
    if let Some(sort) = &params.sort {
        match sort {
            SortDir::Asc => years.sort(),
            SortDir::Desc => years.sort_by(|a, b| b.cmp(a)),
        }
    }
    if let Some(start) = params.start {
        years.retain(|year| year >= &(start.year() as u32));
    }
    if let Some(end) = params.end {
        years.retain(|year| year <= &(end.year() as u32));
    }

    // Create a channel to send log lines
    let (tx, rx) = mpsc::channel(10);

    tokio::spawn(async move {
        let mut count = 0;
        'year_loop: for year in years {
            let mut months = get_monts(year);
            if let Some(sort) = &params.sort {
                match sort {
                    SortDir::Asc => months.sort(),
                    SortDir::Desc => months.sort_by(|a, b| b.cmp(a)),
                }
            }
            if let Some(start) = params.start {
                months.retain(|month| month >= &(start.month() as u32));
            }
            if let Some(end) = params.end {
                months.retain(|month| month <= &(end.month() as u32));
            }

            for month in months {
                let mut days = get_days(year, month);
                if let Some(sort) = &params.sort {
                    match sort {
                        SortDir::Asc => days.sort(),
                        SortDir::Desc => days.sort_by(|a, b| b.cmp(a)),
                    }
                }
                if let Some(start) = params.start {
                    days.retain(|day| day >= &(start.day() as u32));
                }
                if let Some(end) = params.end {
                    days.retain(|day| day <= &(end.day() as u32));
                }

                for day in days {
                    let files = read_dir(logs_path.join(year.to_string()).join(month.to_string()).join(day.to_string())).unwrap();
                    for file in files {
                        let mut file = fs::File::open(file.unwrap().path()).await.unwrap();
                        let mut contents = String::new();
                        file.read_to_string(&mut contents).await.unwrap();

                        for line in contents.lines() {
                            let logline = logline::parse_logline(line);
                            println!("{:?}", logline);

                            if tx.send(Ok(Event::default().data(serde_json::to_string(&logline).unwrap()))).await.is_err() {
                                break 'year_loop;
                            }

                            count += 1;

                            if let Some(limit) = params.count {
                                if count >= limit {
                                    break 'year_loop;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // Return the stream
    Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx))
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