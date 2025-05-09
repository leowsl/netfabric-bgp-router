use serde::{Deserialize, Serialize};
use crate::utils::message_bus::Message;
use crate::components::advertisment::AdvertisementType;
use crate::components::route::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Announcement {
    pub next_hop: String,
    pub prefixes: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Notification {
    pub code: u64,
    pub subcode: u64,
    pub data: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RisLiveData {
    pub timestamp: f64,
    pub peer: String,
    pub peer_asn: String,
    pub id: String,
    pub raw: Option<String>,
    pub host: String,
    #[serde(rename = "type")]
    pub msg_type: AdvertisementType,

    // BGP update
    pub path: Option<Path>,
    pub community: Option<Vec<Vec<u64>>>,
    pub origin: Option<String>,
    pub med: Option<u64>,
    pub aggregator: Option<String>, 
    pub announcements: Option<Vec<Announcement>>,
    pub withdrawals: Option<Vec<String>>,

    // BGP open
    pub direction: Option<String>,
    pub version: Option<u8>,
    pub asn: Option<u64>,
    pub hold_time: Option<u64>,
    pub router_id: Option<String>,
    #[serde(skip)]
    pub capabilities: Option<String>,

    // BGP notification
    pub notification: Option<Notification>,

    // BGP state change
    pub state: Option<String>,
}
impl Message for RisLiveData {}

#[derive(Serialize, Deserialize, Debug)]
pub struct RisLiveMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: RisLiveData,
}
impl Message for RisLiveMessage {}
