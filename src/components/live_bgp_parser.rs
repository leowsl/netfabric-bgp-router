use std::str;
use reqwest::get;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;
use thiserror::Error;

static RIS_STREAM_URL: &str = "https://ris-live.ripe.net/v1/stream/?format=json&client=Netfabric-Test";

#[derive(Error, Debug)]
pub enum BgpParserError {
    #[error("Failed to fetch stream: {0}")]
    StreamError(#[from] reqwest::Error),
    #[error("Failed to parse message: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("Failed to send message: {0}")]
    SendError(#[from] std::sync::mpsc::SendError<RisLiveMessage>),
}

pub async fn start_stream(sender: Sender<RisLiveMessage>) -> Result<(), BgpParserError> {
    let mut stream = get(RIS_STREAM_URL).await?.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        for byte in chunk? {
            if byte == b'\n' {
                if let Ok(msg) = serde_json::from_str(&buf) {
                    sender.send(msg)?;
                }
                buf.clear();
            } else {
                buf.push(byte as char);
            }
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Announcement {
    pub next_hop: String,
    pub prefixes: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RisLiveData {
    pub timestamp: f64,
    pub peer: String,
    pub peer_asn: String,
    pub id: String,
    pub host: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub path: Option<Vec<u64>>,
    pub community: Option<Vec<Vec<u64>>>,
    pub origin: Option<String>,
    pub announcements: Option<Vec<Announcement>>,
    pub raw: Option<String>,
    pub withdrawals: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RisLiveMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: RisLiveData,
}