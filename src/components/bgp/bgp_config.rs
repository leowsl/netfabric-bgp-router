use std::net::{IpAddr, Ipv4Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfig {
    pub as_number: u64,
    pub ip: IpAddr,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            as_number: 0,
            ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        }
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessConfig {
    pub next_hop_self: bool,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            next_hop_self: false,
        }
    }
}