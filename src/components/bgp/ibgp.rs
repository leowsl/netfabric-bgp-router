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
}

impl BgpSessionTrait for IBgpSession {
    fn new(config: SessionConfig) -> Self {
        Self {
            session_type: IBgpSessionType::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::bgp::bgp_session::SessionType;

    #[test]
    fn create_ibgp_session() {
        let config = SessionConfig {
            session_type: SessionType::IBgp,
            as_number: 0,
        };
        let _session = IBgpSession::new(config);
    }
}
