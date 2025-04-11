use std::str;
use reqwest::get;
use futures_util::StreamExt;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::thread;

static RIS_STREAM_URL: &str = "https://ris-live.ripe.net/v1/stream/?format=json&client=Netfabric-Test";

pub async fn start_stream() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf: String = String::new();
    let mut messages: Vec<RisLiveMessage> = Vec::new();

    let mut stream = get(RIS_STREAM_URL)
        .await?
        .bytes_stream();

    while let Some(chunk) = stream.next().await {
        for byte in chunk? {
            if byte == '\n' as u8 {
                // Line is complete
                if let Ok(msg) = deserialize_line(&buf) {
                    messages.push(msg);
                }

                // Flush buffer
                drop(buf);
                buf = String::new();
            }
            else {
                buf.push(byte as char);
            }
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
struct Announcement {
    next_hop: String,
    prefixes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct RisLiveData {
    timestamp: f64,
    peer: String,
    peer_asn: String,
    id: String,
    host: String,
    #[serde(rename = "type")]
    msg_type: String,
    path: Option<Vec<u64>>,
    community: Option<Vec<Vec<u64>>>,
    origin: Option<String>,
    announcements: Option<Vec<Announcement>>,
    raw: Option<String>,
    withdrawals: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
struct RisLiveMessage {
    #[serde(rename = "type")]
    msg_type: String,
    data: RisLiveData,
}

fn deserialize_line(line: &String) -> Result<RisLiveMessage, Box<dyn std::error::Error>> {
    let message: RisLiveMessage = serde_json::from_str(line.as_str())?;
    return Ok(message);
}