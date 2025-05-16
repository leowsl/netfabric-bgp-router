use crate::components::advertisement::Advertisement;
use crate::components::bgp_rib::BgpRib;
use crate::components::interface::Interface;
use crate::utils::message_bus::MessageBusError;
use crate::utils::message_bus::{MessageReceiver, MessageSender};
use crate::utils::mutex_utils::TryLockWithTimeout;
use crate::utils::state_machine::{State, StateTransition};
use log::{error, warn};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};
use thiserror::Error;
use uuid::Uuid;

pub enum RouterChannel {
    Inbound(MessageReceiver),
    Outbound(MessageSender),
}

#[derive(Clone, PartialEq, Eq)]
pub struct RouterOptions {
    pub use_bgp_rib: bool,
    pub capacity: usize,
    pub drop_incoming_advertisements: bool, // If true, this router will act as a blackhole
}
impl Default for RouterOptions {
    fn default() -> Self {
        Self {
            use_bgp_rib: true,
            capacity: 1000,
            drop_incoming_advertisements: false,
        }
    }
}

pub struct Router {
    pub id: Uuid,
    pub options: RouterOptions,
    interfaces: Vec<Interface>,
    bgp_rib: Option<Arc<Mutex<BgpRib>>>,
    incoming_advertisements: Vec<Advertisement>,
    outgoing_advertisements: Vec<Advertisement>,
}

impl Router {
    pub fn new(id: Uuid) -> Self {
        let options = RouterOptions::default();
        Router {
            id,
            bgp_rib: None,
            incoming_advertisements: Vec::with_capacity(options.capacity),
            outgoing_advertisements: Vec::with_capacity(options.capacity),
            interfaces: Vec::new(),
            options,
        }
    }

    pub fn set_options(&mut self, options: RouterOptions) {
        if options.capacity < self.options.capacity {
            if self.incoming_advertisements.len() > options.capacity {
                warn!(
                    "[{:.5}] Decreasing capacity leads to loss of incoming advertisements.",
                    self.id
                );
                self.incoming_advertisements.truncate(options.capacity);
            }
            if self.outgoing_advertisements.len() > options.capacity {
                warn!(
                    "[{:.5}] Decreasing capacity leads to loss of outgoing advertisements.",
                    self.id
                );
                self.outgoing_advertisements.truncate(options.capacity);
            }
            self.incoming_advertisements.shrink_to(options.capacity);
            self.outgoing_advertisements.shrink_to(options.capacity);
        } else if options.capacity > self.options.capacity {
            let additional_capacity = options.capacity - self.options.capacity;
            self.incoming_advertisements.reserve(additional_capacity);
            self.outgoing_advertisements.reserve(additional_capacity);
        }
        self.options = options;
    }

    pub fn new_with_options(id: Uuid, options: RouterOptions) -> Self {
        let mut router = Self::new(id);
        router.set_options(options);
        return router;
    }

    pub fn add_interface(&mut self, interface: Interface) {
        self.interfaces.push(interface);
    }

    pub fn get_interface(&self, id: &Uuid) -> Option<&Interface> {
        self.interfaces.iter().find(|interface| interface.id == *id)
    }

    pub fn get_interface_by_index(&self, index: usize) -> Option<&Interface> {
        self.interfaces.get(index)
    }

    pub fn get_interface_mut(&mut self, id: &Uuid) -> Option<&mut Interface> {
        self.interfaces
            .iter_mut()
            .find(|interface| interface.id == *id)
    }

    pub fn get_interface_by_index_mut(&mut self, index: usize) -> Option<&mut Interface> {
        self.interfaces.get_mut(index)
    }

    pub fn set_rib(&mut self, rib: &Arc<Mutex<BgpRib>>) {
        self.bgp_rib = Some(rib.clone());
        rib.lock().unwrap().register_router(&self.id);
    }

    pub fn get_incoming_advertisements(&mut self) -> Result<(), RouterError> {
        for interface in &mut self.interfaces {
            interface.receive().unwrap_or_else(|e| {
                warn!("[{:.5}] {e}", self.id);
            });
            self.incoming_advertisements
                .extend(interface.get_incoming_advertisements());
        }
        Ok(())
    }

    pub fn update_rib(&mut self) -> Result<(), RouterError> {
        let mut announcements = Vec::new();
        let mut withdrawals = Vec::new();

        for ad in self.incoming_advertisements.clone() {
            announcements.extend(ad.get_announcements());
            withdrawals.extend(ad.get_withdrawals());
        }

        self.bgp_rib
            .as_ref()
            .ok_or_else(|| RouterError::RibNotSet(self.id.to_string()))?
            .try_lock_with_timeout(std::time::Duration::from_millis(100))?
            .update_routes(&announcements, &withdrawals, &self.id);
        Ok(())
    }

    // TODO: Implement this
    pub fn get_best_route_changes(&mut self) -> Result<Vec<Advertisement>, RouterError> {
        return Ok(Vec::new());
    }

    pub fn process_outgoing_advertisements(&mut self) -> Result<(), RouterError> {
        for interface in &mut self.interfaces {
            interface
                .push_outgoing_advertisements(self.outgoing_advertisements.clone())
                .unwrap_or_else(|e| {
                    warn!("[{:.5}] {e}", self.id);
                });
            interface.send();
        }
        self.outgoing_advertisements.clear();
        Ok(())
    }
}

impl State for Router {
    fn work(&mut self) -> StateTransition {
        // Store incoming advertisements in self.incoming_advertisements
        match self.get_incoming_advertisements() {
            Ok(_) => {}
            Err(e) => {
                warn!("[{:.5}] Error processing incoming advertisements: {}", self.id, e);
            }
        };

        if self.options.drop_incoming_advertisements {
            self.incoming_advertisements.clear();
            return StateTransition::Continue;
        }

        // If rib is not enabled, no advertisements will be sent
        if self.options.use_bgp_rib {
            // Update self.rib
            match self.update_rib() {
                Ok(_) => {}
                Err(e) => {
                    warn!("[{:.5}] Error updating RIB: {}", self.id, e);
                }
            };
        } else {
            // If RIB is not enabled, we clear the buffer to not overflow
            self.incoming_advertisements.clear();
        }

        // TODO: Implement this
        // match self.get_best_route_changes() {
        //     Ok(_) => {},    // TODO: Fix when implemented
        //     Err(e) => {
        //         warn!("Error getting best route changes: {}", e);
        //     }
        // };

        // TODO: remove this hack when get_best_route_changes is implemented
        self.outgoing_advertisements.clear();
        self.outgoing_advertisements = self.incoming_advertisements.drain(..).collect();

        // Send outgoing_advertisements
        match self.process_outgoing_advertisements() {
            Ok(_) => (),
            Err(e) => {
                warn!("[{:.5}] Error processing outgoing advertisements: {}", self.id, e);
            }
        };

        // Make sure the buffers are empty
        self.incoming_advertisements.clear();
        self.outgoing_advertisements.clear();

        StateTransition::Continue
    }
}

#[derive(Error, Debug)]
pub enum RouterError {
    #[error("Message bus error: {0}")]
    MessageBusError(String),
    #[error("Failed to lock resource: {0}")]
    LockError(String),
    #[error("Failed to process message: {0}")]
    ProcessError(String),
    #[error("RIB is not set for router {0}")]
    RibNotSet(String),
    #[error("Interface error: {0}")]
    InterfaceError(String),
}

impl From<MessageBusError> for RouterError {
    fn from(err: MessageBusError) -> Self {
        RouterError::MessageBusError(err.to_string())
    }
}

impl<T> From<PoisonError<T>> for RouterError {
    fn from(err: PoisonError<T>) -> Self {
        RouterError::LockError(err.to_string())
    }
}

impl<T> From<TryLockError<T>> for RouterError {
    fn from(err: TryLockError<T>) -> Self {
        RouterError::LockError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::advertisement::{AdvertisementType, Announcement};
    use crate::components::route::PathElement;
    use crate::utils::state_machine::{StateMachine, StateMachineError};
    use crate::utils::thread_manager::ThreadManager;
    use ip_network::IpNetwork;
    use std::net::IpAddr;
    use std::net::Ipv4Addr;

    #[test]
    fn test_create_router() {
        let router = Router::new(Uuid::new_v4());
        assert!(router.id != Uuid::nil());
    }

    #[test]
    fn test_router_set_rib() {
        use crate::utils::router_mask::RouterMask;

        let id = Uuid::new_v4();
        let mut router = Router::new(id.clone());
        let rib = Arc::new(Mutex::new(BgpRib::new()));
        router.set_rib(&rib);
        assert!(router.bgp_rib.is_some());
        assert_eq!(
            router
                .bgp_rib
                .unwrap()
                .lock()
                .unwrap()
                .get_router_mask_map()
                .get_all(),
            &RouterMask(0b1)
        );
    }

    #[test]
    fn test_router_state() -> Result<(), StateMachineError> {
        let mut thread_manager: ThreadManager = ThreadManager::new();
        let router = Router::new(Uuid::new_v4());
        let mut state_machine = StateMachine::new(&mut thread_manager, router)?;
        state_machine.start()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        state_machine.stop()?;
        Ok(())
    }

    #[test]
    fn test_router_interface() -> Result<(), StateMachineError> {
        let mut thread_manager: ThreadManager = ThreadManager::new();
        let router_id = Uuid::new_v4();
        let mut router = Router::new(router_id.clone());

        // Create and set up interface
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let (tx1, rx1) = thread_manager.get_message_bus_channel_pair(1000)?;
        let (tx2, rx2) = thread_manager.get_message_bus_channel_pair(1000)?;
        interface.set_in_channel(rx1);
        interface.set_out_channel(tx2);
        router.add_interface(interface);

        let mut state_machine = StateMachine::new(&mut thread_manager, router)?;
        state_machine.start()?;

        let advertisement = Advertisement {
            timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
            peer: "192.168.1.1".to_string(),
            ..Default::default()
        };
        tx1.send(Box::new(advertisement.clone())).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(500));

        assert_eq!(
            rx2.try_recv().unwrap().cast::<Advertisement>().unwrap(),
            &advertisement
        );
        state_machine.stop()?;
        Ok(())
    }

    #[test]
    fn test_router_bgp_update() -> Result<(), StateMachineError> {
        let mut thread_manager: ThreadManager = ThreadManager::new();
        let router_id = Uuid::new_v4();
        let mut router = Router::new(router_id.clone());

        // Create and set up BGP RIB
        let bgp_rib = Arc::new(Mutex::new(BgpRib::new()));
        router.set_rib(&bgp_rib);

        // Create and set up interface
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let (tx1, rx1) = thread_manager.get_message_bus_channel_pair(1000)?;
        interface.set_in_channel(rx1);
        router.add_interface(interface);

        let mut state_machine = StateMachine::new(&mut thread_manager, router)?;
        state_machine.start()?;

        let advertisement = Advertisement {
            timestamp: 0.0,
            peer: "192.168.1.1".to_string(),
            peer_asn: "1".to_string(),
            id: "test".to_string(),
            host: "test".to_string(),
            msg_type: AdvertisementType::Update,
            path: Some(vec![PathElement::ASN(1), PathElement::ASN(2)]),
            community: None,
            origin: None,
            announcements: Some(vec![Announcement {
                next_hop: "192.168.1.1".to_string(),
                prefixes: vec!["192.168.1.0/24".to_string()],
            }]),
            raw: None,
            withdrawals: None,
            ..Default::default()
        };
        tx1.send(Box::new(advertisement.clone())).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(200));
        state_machine.stop()?;

        let rib = bgp_rib.lock().unwrap();
        assert_eq!(rib.get_prefix_count(), (1, 0));
        let routes =
            rib.get_routes_for_router(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)), &router_id);
        assert_eq!(routes.len(), 1);
        assert_eq!(
            routes[0].prefix,
            IpNetwork::new(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap()
        );
        assert_eq!(routes[0].next_hop, "192.168.1.1");
        assert_eq!(
            routes[0].as_path,
            vec![PathElement::ASN(1), PathElement::ASN(2)]
        );
        Ok(())
    }
}
