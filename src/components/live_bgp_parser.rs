use std::str;
use reqwest::get;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;

static RIS_STREAM_URL: &str = "https://ris-live.ripe.net/v1/stream/?format=json&client=Netfabric-Test";

pub async fn start_stream(sender: Sender<RisLiveMessage>) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf: String = String::new();

    let mut stream = get(RIS_STREAM_URL)
        .await?
        .bytes_stream();

    while let Some(chunk) = stream.next().await {
        for byte in chunk? {
            if byte == '\n' as u8 {
                // Line is complete
                if let Ok(msg) = deserialize_line(&buf) {
                    sender.send(msg).unwrap();
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

fn deserialize_line(line: &String) -> Result<RisLiveMessage, Box<dyn std::error::Error>> {
    let message: RisLiveMessage = serde_json::from_str(line.as_str())?;
    return Ok(message);
}