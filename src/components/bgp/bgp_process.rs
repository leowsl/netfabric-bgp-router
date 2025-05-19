use crate::components::advertisement::Advertisement;
use crate::components::bgp::{
    bgp_config::{ProcessConfig, SessionConfig},
    bgp_rib::BgpRibInterface,
    bgp_session::BgpSession,
};
use crate::components::bgp_rib::BgpRib;
use log::error;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct BgpProcess {
    pub id: Uuid,
    pub sessions: Vec<BgpSession>,
    pub config: ProcessConfig,
    pub rib: BgpRibInterface,
}

impl BgpProcess {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            sessions: Vec::new(),
            config: ProcessConfig::default(),
            // it's not the prettiest solution, but if we set a new rib, the old one will dropped as it's arc has 0 references
            rib: BgpRibInterface::new(Arc::new(Mutex::new(BgpRib::new())), Uuid::new_v4()),
        }
    }

    pub fn set_rib(&mut self, rib: BgpRibInterface) {
        self.rib = rib;
    }

    pub fn with_rib(mut self, rib: BgpRibInterface) -> Self {
        self.set_rib(rib);
        return self;
    }

    pub fn set_config(&mut self, config: ProcessConfig) {
        self.config = config;
    }

    pub fn with_config(mut self, config: ProcessConfig) -> Self {
        self.set_config(config);
        return self;
    }

    pub fn from_config(config: ProcessConfig) -> Self {
        let mut process = Self::new();
        process.set_config(config);
        return process;
    }

    pub fn create_session(&mut self, config: SessionConfig) {
        let session = BgpSession::from_config(config);
        self.add_session(session);
    }

    fn add_session(&mut self, session: BgpSession) {
        self.sessions.push(session);
    }

    fn remove_session(&mut self, session_id: &Uuid) {
        self.sessions.retain(|session| &session.id != session_id);
    }

    fn sessions_iter(&self) -> impl Iterator<Item = &BgpSession> {
        self.sessions.iter()
    }

    fn sessions_iter_mut(&mut self) -> impl Iterator<Item = &mut BgpSession> {
        self.sessions.iter_mut()
    }

    pub fn receive(&mut self) {
        let all_advertisements: Vec<Advertisement> = self
            .sessions_iter_mut()
            .filter_map(|session| match session.receive() {
                Ok(advertisements) => Some(advertisements),
                Err(e) => {
                    error!("[{:.4}] Error receiving advertisements: {:?}", session.id, e);
                    None
                }
            })
            .flatten()
            .collect();
        self.rib.update(all_advertisements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{
        bgp::{bgp_config::SessionConfig, bgp_session::BgpSession},
        interface::Interface,
    };
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_add_remove_session() {
        let mut bgp_process = BgpProcess::new();
        let config = SessionConfig {
            as_number: 1,
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            ..Default::default()
        };
        let session = BgpSession::from_config(config);
        let session_id = session.id.clone();

        // Add
        bgp_process.add_session(session);
        assert_eq!(bgp_process.sessions.len(), 1);
        assert_eq!(bgp_process.sessions[0].id, session_id);

        // Remove
        bgp_process.remove_session(&session_id);
        assert_eq!(bgp_process.sessions.len(), 0);
    }

    #[test]
    fn test_sessions_iter() {
        let mut bgp_process = BgpProcess::new();
        let config = SessionConfig {
            as_number: 1,
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            ..Default::default()
        };

        // Create sessionsc
        let session1 = BgpSession::from_config(config.clone());
        let session2 = BgpSession::from_config(config.clone());

        let session1_id = session1.id.clone();
        let session2_id = session2.id.clone();

        bgp_process.add_session(session1);
        bgp_process.add_session(session2);

        // Iter mut
        for session in bgp_process.sessions_iter_mut() {
            let iface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)));
            session.set_interface(iface);
        }

        // Iter
        let sessions = bgp_process.sessions_iter().collect::<Vec<_>>();

        // Assert
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, session1_id);
        assert_eq!(sessions[1].id, session2_id);
        assert!(sessions[0].get_interface().is_some());
        assert!(sessions[1].get_interface().is_some());
    }
}
