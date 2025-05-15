use crate::components::bgp::bgp_session::BgpSessionTrait;
use crate::components::bgp::bgp_config::SessionConfig;

#[derive(Debug, Clone)]
pub struct EBgpSession {}

impl BgpSessionTrait for EBgpSession {
    fn new(config: SessionConfig) -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::bgp::bgp_session::SessionType;

    #[test]
    fn create_ebgp_session() {
        let config = SessionConfig {
            session_type: SessionType::EBgp,
            as_number: 0,
        };
        let _session = EBgpSession::new(config);
    }
}
