use std::net::IpAddr;

use crate::components::bgp::bgp_session::BgpSessionTrait;
use crate::components::bgp::bgp_config::SessionConfig;

#[derive(Debug, Clone)]
pub struct EBgpSession {
    ip: IpAddr,
    as_number: u64,
}

impl BgpSessionTrait for EBgpSession {
    fn new(config: SessionConfig) -> Self {
        Self {
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
    fn create_ebgp_session() {
        let config = SessionConfig {
            session_type: SessionType::EBgp,
            as_number: 0,
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        };
        let _session = EBgpSession::new(config);
    }
}
