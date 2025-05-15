use std::net::IpAddr;

use crate::components::bgp::bgp_session::BgpSessionTrait;
use crate::components::bgp::bgp_config::SessionConfig;

#[derive(Debug, Clone)]
pub struct EBgpSession {
    ip: IpAddr,
}

impl BgpSessionTrait for EBgpSession {
    fn new(config: SessionConfig) -> Self {
        Self {
            ip: config.ip,
        }
    }

    fn get_session_ip(&self) -> &IpAddr {
        &self.ip
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
