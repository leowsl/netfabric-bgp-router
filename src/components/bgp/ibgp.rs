use std::net::IpAddr;

use crate::components::bgp::bgp_config::SessionConfig;
use crate::components::bgp::bgp_session::BgpSessionTrait;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum IBgpSessionType {
    #[default]
    Unknown,
    RouteReflector,
    FullMesh,
}

#[derive(Debug, Clone)]
pub struct IBgpSession {
    session_type: IBgpSessionType,
    ip: IpAddr,
    as_number: u64,
}

impl BgpSessionTrait for IBgpSession {
    fn new(config: SessionConfig) -> Self {
        Self {
            session_type: IBgpSessionType::default(),
            ip: config.ip,
            as_number: config.as_number,
        }
    }

    fn get_ip(&self) -> &IpAddr {
        &self.ip
    }

    fn get_as_number(&self) -> u64 {
        self.as_number
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::bgp::bgp_session::SessionType;
    use std::net::Ipv4Addr;

    #[test]
    fn create_ibgp_session() {
        let config = SessionConfig {
            session_type: SessionType::IBgp,
            as_number: 0,
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        };
        let _session = IBgpSession::new(config);
    }
}
