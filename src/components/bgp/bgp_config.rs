use std::net::{IpAddr, Ipv4Addr};

use crate::components::bgp::bgp_session::SessionType;

pub struct SessionConfig {
    pub session_type: SessionType,
    pub as_number: u64,
    pub ip: IpAddr,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_type: SessionType::IBgp,
            as_number: 0,
            ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        }
    }
}