use crate::components::advertisement::Advertisement;
use crate::components::bgp::bgp_config::SessionConfig;
use crate::components::interface::{Interface, InterfaceError};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum OriginType {
    IGP,
    EGP,
    INCOMPLETE,
}

#[derive(Debug)]
pub struct BgpSession {
    pub id: Uuid,
    config: SessionConfig,
    interface: Option<Interface>,
}

impl BgpSession {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            config: SessionConfig::default(),
            interface: None,
        }
    }

    pub fn set_config(&mut self, config: SessionConfig) {
        self.config = config;
    }

    pub fn from_config(config: SessionConfig) -> Self {
        let mut session = Self::new();
        session.set_config(config);
        return session;
    }

    pub fn get_config(&self) -> &SessionConfig {
        &self.config
    }

    pub fn with_interface(mut self, interface: Interface) -> Self {
        self.set_interface(interface);
        return self;
    }

    pub fn set_interface(&mut self, interface: Interface) {
        self.interface = Some(interface);
    }

    pub fn get_interface(&self) -> Option<&Interface> {
        self.interface.as_ref()
    }

    pub fn get_interface_mut(&mut self) -> Option<&mut Interface> {
        self.interface.as_mut()
    }

    pub fn receive(&mut self) -> Result<Vec<Advertisement>, BgpSessionError> {
        match &mut self.interface {
            None => Err(BgpSessionError::InterfaceNotSet),
            Some(interface) => {
                interface.receive()?;
                Ok(interface.get_incoming_advertisements())
            }
        }
    }

    pub fn send(&mut self, advertisements: Vec<Advertisement>) -> Result<(), BgpSessionError> {
        match &mut self.interface {
            None => Err(BgpSessionError::InterfaceNotSet),
            Some(interface) => {
                let send_result = interface.push_outgoing_advertisements(advertisements);
                interface.send();
                match send_result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(BgpSessionError::InterfaceError(e)),
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BgpSessionError {
    InterfaceNotSet,
    InterfaceError(InterfaceError),
}

impl From<InterfaceError> for BgpSessionError {
    fn from(error: InterfaceError) -> Self {
        BgpSessionError::InterfaceError(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThreadManager;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn create_bgp_session() {
        let config = SessionConfig {
            next_hop_self: true,
            ..Default::default()
        };
        let session = BgpSession::from_config(config.clone());
        assert_eq!(session.config, config);
        assert!(session.interface.is_none());
    }

    #[test]
    fn test_send_receive() {
        let mut thread_manager = ThreadManager::new();
        let mut session = BgpSession::new();
        let interface = Interface::new_loopback(
            &mut thread_manager,
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            100,
            100,
        );
        session.set_interface(interface);

        let ad = Advertisement {
            timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
            peer: "192.168.1.2".to_string(),
            ..Default::default()
        };
        session.send(vec![ad.clone(); 5]).unwrap();
        let rec = session.receive().unwrap();
        assert_eq!(rec.len(), 5);
        for i in 0..5 {
            assert_eq!(rec[i], ad);
        }
    }
}
