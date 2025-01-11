use core::time;
use std::io::{self, Error, Read, Write};
use bytes::Bytes;
use chrono::{DateTime, Timelike, Utc};
use futures::Stream;
use futures_util::StreamExt;

#[derive(Debug, serde::Serialize)]
pub struct Logline {
    timestamp: String,
    loglevel: String,
    message: String
}

// 2025-01-06T21:16:54.279466 INFO device SensorY disconnected
pub fn parse_logline(logline: &str) -> Logline {
    let mut parts = logline.split(" ");
    let timestamp = parts.next().unwrap();
    let loglevel = parts.next().unwrap();
    Logline {
        timestamp: timestamp.to_string(),
        loglevel: loglevel.to_string(),
        message: parts.collect::<Vec<&str>>().join(" ")
    }
}