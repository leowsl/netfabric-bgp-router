use crate::components::bgp::bgp_session::{BgpSessionType, BgpSessionTypeTrait};

#[derive(Debug, Clone)]
pub struct IBgpSession {}

impl IBgpSession {
    pub fn new() -> Self {
        Self {}
    }
}

impl BgpSessionTypeTrait for IBgpSession {
    fn create_session_type() -> BgpSessionType {
        BgpSessionType::IBgp(Self::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_ibgp_session() {
        let _session = IBgpSession::new();
    }
}
