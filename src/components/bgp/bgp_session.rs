use std::net::IpAddr;

use crate::components::bgp::bgp_config::SessionConfig;
use crate::components::bgp::ebgp::EBgpSession;
use crate::components::bgp::ibgp::IBgpSession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionType {
    IBgp,
    EBgp,
}

pub trait BgpSessionTrait {
    fn new(config: SessionConfig) -> Self;
    fn get_session_ip(&self) -> &IpAddr;
}

#[derive(Debug)]
pub enum BgpSession {
    IBgp(IBgpSession),
    EBgp(EBgpSession),
}

impl BgpSession {
    pub fn new(config: SessionConfig) -> Self {
        match config.session_type {
            SessionType::IBgp => Self::IBgp(IBgpSession::new(config)),
            SessionType::EBgp => Self::EBgp(EBgpSession::new(config)),
        }
    }

    pub fn get_session_type(&self) -> SessionType {
        match self {
            Self::IBgp(_) => SessionType::IBgp,
            Self::EBgp(_) => SessionType::EBgp,
        }
    }

    pub fn get_session_ip(&self) -> &IpAddr {
        match self {
            Self::IBgp(session) => session.get_session_ip(),
            Self::EBgp(session) => session.get_session_ip(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn create_bgp_session() {
        let config = SessionConfig {
            session_type: SessionType::IBgp,
            as_number: 0,
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        };
        let session = BgpSession::new(config);
        assert_eq!(session.get_session_type(), SessionType::IBgp);
    }
}
