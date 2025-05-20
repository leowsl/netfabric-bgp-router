use std::net::{IpAddr, Ipv4Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfig {
    pub session_ip: IpAddr,
    pub interface_ip: IpAddr,
    pub next_hop_self: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            interface_ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            next_hop_self: false,
        }
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessConfig {
    pub ip: IpAddr,
    pub as_number: u64,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            as_number: 0,
        }
    }
}