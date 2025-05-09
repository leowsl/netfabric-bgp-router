use serde::{Deserialize, Serialize};
use crate::utils::message_bus::Message;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
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
impl Message for RisLiveData {}

#[derive(Serialize, Deserialize, Debug)]
pub struct RisLiveMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: RisLiveData,
}
impl Message for RisLiveMessage {}
