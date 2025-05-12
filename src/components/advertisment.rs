use crate::components::ris_live_data::{Announcement, RisLiveData};
use crate::components::route::{Route, Path};
use crate::utils::message_bus::Message;
use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "String")]
pub enum AdvertisementType {
    Unknown,
    Update,
    Keepalive,
    Open,
    Notification,
    RisPeerState,
}
impl Default for AdvertisementType {
    fn default() -> Self {
        AdvertisementType::Unknown
    }
}
impl From<String> for AdvertisementType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "UPDATE" => AdvertisementType::Update,
            "KEEPALIVE" => AdvertisementType::Keepalive,
            "OPEN" => AdvertisementType::Open,
            "NOTIFICATION" => AdvertisementType::Notification,
            "RIS_PEER_STATE" | "STATE" => AdvertisementType::RisPeerState,
            _ => {
                warn!("Invalid advertisement type: {}", s);
                AdvertisementType::Unknown
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Advertisement {
    pub timestamp: f64,
    pub peer: String,
    pub peer_asn: String,
    pub id: String,
    pub host: String,
    pub msg_type: AdvertisementType,
    pub path: Option<Path>,
    pub community: Option<Vec<Vec<u64>>>,
    pub origin: Option<String>,
    pub announcements: Option<Vec<Announcement>>,
    pub raw: Option<String>,
    pub withdrawals: Option<Vec<String>>,
}

impl Advertisement {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_routes(&self) -> Vec<Route> {
        let mut routes: Vec<Route> = Vec::new();
        if let Some(announcements) = &self.announcements {
            for announcement in announcements.iter() {
                for prefix in announcement.prefixes.iter() {
                    routes.push(Route {
                        next_hop: announcement.next_hop.clone(),
                        prefix: prefix.clone(),
                        as_path: self.path.clone().unwrap_or_default(),
                    });
                }
            }
        }
        routes
    }

    pub fn get_withdrawals(&self) -> Vec<Route> {
        vec![]
    }
}

impl Message for Advertisement {}

impl From<RisLiveData> for Advertisement {
    fn from(ris_live_data: RisLiveData) -> Self {
        Self {
            timestamp: ris_live_data.timestamp,
            peer: ris_live_data.peer,
            peer_asn: ris_live_data.peer_asn,
            id: ris_live_data.id,
            host: ris_live_data.host,
            msg_type: ris_live_data.msg_type,
            path: ris_live_data.path,
            community: ris_live_data.community,
            origin: ris_live_data.origin,
            announcements: ris_live_data.announcements,
            raw: ris_live_data.raw,
            withdrawals: ris_live_data.withdrawals,
        }
    }
}

impl From<&RisLiveData> for Advertisement {
    fn from(ris_live_data: &RisLiveData) -> Self {
        Self {
            timestamp: ris_live_data.timestamp,
            peer: ris_live_data.peer.clone(),
            peer_asn: ris_live_data.peer_asn.clone(),
            id: ris_live_data.id.clone(),
            host: ris_live_data.host.clone(),
            msg_type: ris_live_data.msg_type.clone(),
            path: ris_live_data.path.clone(),
            community: ris_live_data.community.clone(),
            origin: ris_live_data.origin.clone(),
            announcements: ris_live_data.announcements.clone(),
            raw: ris_live_data.raw.clone(),
            withdrawals: ris_live_data.withdrawals.clone(),
        }
    }
}
