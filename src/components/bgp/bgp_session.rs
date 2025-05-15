use crate::components::bgp::ebgp::EBgpSession;
use crate::components::bgp::ibgp::IBgpSession;

pub struct BgpSession {
    session_type: BgpSessionType,
}

impl BgpSession {
    pub fn new(session_type: BgpSessionType) -> Self {
        Self { session_type }
    }

    pub fn session_type(&self) -> &BgpSessionType {
        &self.session_type
    }
}

#[derive(Debug, Clone)]
pub enum BgpSessionType {
    Bgp,
    IBgp(IBgpSession),
    EBgp(EBgpSession),
}

pub trait BgpSessionTypeTrait {
    fn create_session_type() -> BgpSessionType;
}

impl BgpSessionTypeTrait for BgpSessionType {
    fn create_session_type() -> BgpSessionType {
        BgpSessionType::Bgp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_bgp_session() {
        let session = BgpSession::new(BgpSessionType::Bgp);
        match session.session_type() {
            BgpSessionType::Bgp => (),
            _ => panic!("Expected BgpSessionType::Bgp"),
        }
        match session.session_type {
            BgpSessionType::Bgp => (),
            _ => panic!("Expected BgpSessionType::Bgp"),
        }

        let session = BgpSession::new(BgpSessionType::IBgp(IBgpSession::new()));
        match session.session_type() {
            BgpSessionType::IBgp(_) => (),
            _ => panic!("Expected BgpSessionType::IBgp"),
        }
        match session.session_type {
            BgpSessionType::IBgp(_) => (),
            _ => panic!("Expected BgpSessionType::IBgp"),
        }

        let session = BgpSession::new(BgpSessionType::EBgp(EBgpSession::new()));
        match session.session_type() {
            BgpSessionType::EBgp(_) => (),
            _ => panic!("Expected BgpSessionType::EBgp"),
        }
        match session.session_type {
            BgpSessionType::EBgp(_) => (),
            _ => panic!("Expected BgpSessionType::EBgp"),
        }
    }
}
