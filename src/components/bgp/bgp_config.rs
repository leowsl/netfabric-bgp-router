use crate::components::bgp::bgp_session::SessionType;

pub struct SessionConfig {
    pub session_type: SessionType,
    pub as_number: u64,
}
