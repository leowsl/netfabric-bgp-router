use crate::components::advertisement::{Advertisement, AdvertisementType, Announcement};
use crate::components::bgp::{
    bgp_bestroute::BestRoute,
    bgp_config::{ProcessConfig, SessionConfig},
    bgp_rib::{BgpRib, BgpRibInterface},
    bgp_session::BgpSession,
};
use log::error;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct BgpProcess {
    pub id: Uuid,
    pub sessions: Vec<BgpSession>,
    pub config: ProcessConfig,
    pub rib_interface: BgpRibInterface,
    bestroute_updates: (Vec<BestRoute>, Vec<BestRoute>),
    advertisement: Vec<Advertisement>,
}

impl BgpProcess {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            sessions: Vec::new(),
            config: ProcessConfig::default(),
            // it's not the prettiest solution, but if we set a new rib, the old one will dropped as it's arc has 0 references
            rib_interface: BgpRibInterface::new(Arc::new(Mutex::new(BgpRib::new())), Uuid::new_v4()),
            bestroute_updates: (Vec::new(), Vec::new()),
            advertisement: Vec::new(),
        }
    }

    pub fn set_router_id(&mut self, id: &Uuid) {
        self.rib_interface.set_client_id(id);
    }

    pub fn with_router_id(mut self, id: &Uuid) -> Self {
        self.set_router_id(id);
        return self;
    }

    pub fn set_rib(&mut self, rib: Arc<Mutex<BgpRib>>) {
        self.rib_interface.set_rib(rib);
    }

    pub fn with_rib(mut self, rib: Arc<Mutex<BgpRib>>) -> Self {
        self.set_rib(rib);
        return self;
    }

    pub fn set_rib_interface(&mut self, rib: BgpRibInterface) {
        self.rib_interface = rib;
    }

    pub fn with_rib_interface(mut self, rib: BgpRibInterface) -> Self {
        self.set_rib_interface(rib);
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
                    error!(
                        "[{:.4}] Error receiving advertisements: {:?}",
                        session.id, e
                    );
                    None
                }
            })
            .flatten()
            .collect();
        self.rib_interface.update(all_advertisements);
    }

    pub fn get_best_route_changes(&mut self) {
        self.bestroute_updates = self.rib_interface.get_bestroute_updates();
    }

    pub fn generate_advertisement(&mut self) {
        let announcements = self
            .bestroute_updates
            .0
            .iter()
            .map(|route| Announcement {
                next_hop: route.0.next_hop.clone(),
                prefixes: vec![route.0.prefix.to_string()],
            })
            .collect();
        let withdrawals: Vec<String> = self
            .bestroute_updates
            .1
            .iter()
            .map(|route| route.0.prefix.to_string())
            .collect();
        let ad = Advertisement {
            timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
            peer: self.config.ip.to_string(),
            peer_asn: self.config.as_number.to_string(),
            id: self.id.to_string(),
            host: self.config.ip.to_string(),
            path: None,
            community: None,
            origin: None,
            announcements: Some(announcements),
            withdrawals: Some(withdrawals),
            msg_type: AdvertisementType::Update,
            raw: Some("".to_string()),
        };
        self.advertisement.push(ad);
    }

    pub fn send(&mut self) {
        let advertisement = self.advertisement.clone();
        self.sessions_iter_mut().for_each(|session| {
            session
                .send(advertisement.clone())
                .unwrap_or_else(|e| error!("Error sending advertisement: {:?}", e));
        });
        self.advertisement.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        components::{
            bgp::{bgp_config::SessionConfig, bgp_session::BgpSession},
            interface::Interface,
            route::PathElement,
        },
        ThreadManager,
    };
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_add_remove_session() {
        let mut bgp_process = BgpProcess::new();
        let config = SessionConfig::default();
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
        let config = SessionConfig::default();

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

    #[test]
    fn test_bgp_process_flow() {
        // Create BGP process with config
        let mut bgp_process = BgpProcess::new();
        let process_config = ProcessConfig {
            as_number: 1,
            ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            ..Default::default()
        };
        bgp_process.set_config(process_config);

        // Create session with loopback interface
        let session_config = SessionConfig::default();
        let mut session = BgpSession::from_config(session_config);
        let loopback = Interface::new_loopback(
            &mut ThreadManager::new(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            1024,
            1024,
        );
        session.set_interface(loopback);
        bgp_process.add_session(session);

        // Create initial advertisement
        let initial_ad = Advertisement {
            timestamp: 0.0,
            peer: "10.0.0.2".to_string(),
            peer_asn: "2".to_string(),
            id: Uuid::new_v4().to_string(),
            host: "10.0.0.2".to_string(),
            path: Some(vec![PathElement::ASN(2), PathElement::ASN(10)]),
            community: None,
            origin: None,
            announcements: Some(vec![Announcement {
                next_hop: "192.168.1.2".to_string(),
                prefixes: vec!["1.0.0.0/24".to_string()],
            }]),
            withdrawals: None,
            msg_type: AdvertisementType::Update,
            raw: Some("".to_string()),
        };

        // Add advertisement and send
        bgp_process.advertisement.push(initial_ad);
        bgp_process.send();
        assert!(bgp_process.advertisement.is_empty());

        // Receive and process
        bgp_process.receive();
        bgp_process.get_best_route_changes();
        bgp_process.generate_advertisement();

        // Verify generated advertisement
        assert!(!bgp_process.advertisement.is_empty());
        let generated_ad = &bgp_process.advertisement[0];
        assert_eq!(generated_ad.peer, "10.0.0.1");
        assert_eq!(generated_ad.peer_asn, "1");

        if let Some(announcements) = &generated_ad.announcements {
            assert!(!announcements.is_empty());
            assert_eq!(announcements[0].next_hop, "192.168.1.2");
            assert_eq!(announcements[0].prefixes[0], "1.0.0.0/24");
        }
    }
}
