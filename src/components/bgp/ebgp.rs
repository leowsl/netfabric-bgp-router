use crate::components::bgp::bgp_session::{BgpSessionType, BgpSessionTypeTrait};

#[derive(Debug, Clone)]
pub struct EBgpSession {}

impl EBgpSession {
    pub fn new() -> Self {
        Self {}
    }
}

impl BgpSessionTypeTrait for EBgpSession {
    fn create_session_type() -> BgpSessionType {
        BgpSessionType::EBgp(Self::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_ebgp_session() {
        let _session = EBgpSession::new();
    }
}
