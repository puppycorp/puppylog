use std::io::Write;

use axum::{
    body::{Body, BodyDataStream}, extract::{DefaultBodyLimit, Path}, http::StatusCode, routing::{get, post}, Json, Router
};
use chrono::{Datelike, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tower_http::decompression::{DecompressionLayer, RequestDecompressionLayer};

mod parsing;

fn log_path() -> std::path::PathBuf {
    match std::env::var("LOG_PATH") {
        Ok(val) => std::path::Path::new(&val).to_owned(),
        Err(_) => std::path::Path::new("./logs").to_owned()
    }
}

#[tokio::main]
async fn main() {
    // initialize tracing
    //tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        .route("/", get(root))
        .route("/api/device/{devid}/rawlogs", post(upload_raw_logs))
            .layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
            .layer(RequestDecompressionLayer::new().gzip(true))
        .route("/api/device/{devid}/rawlogs/stream", post(stream_raw_logs))
            .layer(DefaultBodyLimit::max(1024 * 1024 * 1000))
            .layer(RequestDecompressionLayer::new().gzip(true));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
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

#[axum::debug_handler]
async fn create_user(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    // insert your application logic here
    let user = User {
        id: 1337,
        username: payload.username,
    };

    // this will be converted into a JSON response
    // with a status code of `201 Created`
    (StatusCode::CREATED, Json(user))
}

// the input to our `create_user` handler
#[derive(Deserialize)]
struct CreateUser {
    username: String,
}

// the output to our `create_user` handler
#[derive(Serialize)]
struct User {
    id: u64,
    username: String,
}