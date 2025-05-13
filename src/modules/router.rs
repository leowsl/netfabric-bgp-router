use crate::components::advertisement::Advertisement;
use crate::components::bgp_rib::BgpRib;
use crate::components::filters::Filter;
use crate::components::route::Route;
use crate::utils::filter_utils::apply_filters;
use crate::utils::message_bus::MessageBusError;
use crate::utils::message_bus::{MessageReceiver, MessageSender};
use crate::utils::mutex_utils::TryLockWithTimeout;
use crate::utils::state_machine::{State, StateTransition};
use log::{info, warn};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};
use thiserror::Error;
use uuid::Uuid;

pub enum RouterChannel {
    Inbound(MessageReceiver),
    Outbound(MessageSender),
}

pub struct RouterConnection {
    pub channel: RouterChannel,
    pub filters: Vec<Box<dyn Filter<Advertisement>>>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RouterOptions {
    pub use_bgp_rib: bool,
}
impl Default for RouterOptions {
    fn default() -> Self {
        Self { use_bgp_rib: true }
    }
}

pub struct Router {
    pub id: Uuid,
    pub options: RouterOptions,
    connections: Vec<Arc<Mutex<RouterConnection>>>,
    bgp_rib: Option<Arc<Mutex<BgpRib>>>,
}

impl Router {
    pub fn new(id: Uuid) -> Self {
        Router {
            id,
            options: Default::default(),
            connections: Vec::new(),
            bgp_rib: None,
        }
    }

    pub fn set_options(&mut self, options: RouterOptions) {
        self.options = options;
    }

    pub fn new_with_options(id: Uuid, options: RouterOptions) -> Self {
        let mut router = Self::new(id);
        router.set_options(options);
        return router;
    }

    pub fn add_connection(&mut self, connection: RouterConnection) {
        self.connections.push(Arc::new(Mutex::new(connection)));
    }

    pub fn set_rib(&mut self, rib: &Arc<Mutex<BgpRib>>) {
        self.bgp_rib = Some(rib.clone());
        rib.lock().unwrap().register_router(&self.id);
    }

    pub fn get_incoming_advertisements(&mut self) -> Result<Vec<Advertisement>, RouterError> {
        const MAX_BATCH_SIZE: usize = 1000;
        let mut advertisements: Vec<Advertisement> = Vec::with_capacity(MAX_BATCH_SIZE);
        for connection_mutex in &self.connections {
            let connection = connection_mutex.try_lock()?;
            match &connection.channel {
                RouterChannel::Outbound(_) => continue,
                RouterChannel::Inbound(receiver) => {
                    while let Ok(msg) = receiver.try_recv() {
                        if let Some(mut ad) = msg.cast::<Advertisement>().cloned() {
                            if apply_filters(&mut ad, &connection.filters).is_some() {
                                advertisements.push(ad);
                                if advertisements.len() >= MAX_BATCH_SIZE {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        if self.options.use_bgp_rib {
            let announcements: Vec<Route> = advertisements
                .iter()
                .flat_map(|ad| ad.get_announcements())
                .collect();
            let withdrawals: Vec<Route> = advertisements
                .iter()
                .flat_map(|ad| ad.get_withdrawals())
                .collect();
            self.bgp_rib
                .as_ref()
                .ok_or_else(|| RouterError::RibNotSet(self.id.to_string()))?
                .try_lock_with_timeout(std::time::Duration::from_millis(100))?
                .update_routes(&announcements, &withdrawals, &self.id);
        }
        Ok(advertisements)
    }

    pub fn process_outgoing_advertisements(
        &mut self,
        advertisements: Vec<Advertisement>,
    ) -> Result<(), RouterError> {
        for connection_mutex in &self.connections {
            let connection = connection_mutex.try_lock()?;
            match &connection.channel {
                RouterChannel::Inbound(_) => continue,
                RouterChannel::Outbound(sender) => {
                    for advertisement in &advertisements {
                        sender
                            .try_send(Box::new(advertisement.clone()))
                            .map_err(|e| RouterError::MessageBusError(e.to_string()))?;
                    }
                }
            }
        }
        Ok(())
    }
}
impl State for Router {
    fn work(&mut self) -> StateTransition {
        // Process incoming advertisements
        let routes = match self.get_incoming_advertisements() {
            Ok(routes) => routes,
            Err(e) => {
                warn!("Error processing incoming advertisements: {}", e);
                return StateTransition::Continue;
            }
        };

        // Process outgoing advertisements
        match self.process_outgoing_advertisements(routes) {
            Ok(_) => (),
            Err(e) => {
                warn!("Error processing outgoing advertisements: {}", e);
                return StateTransition::Continue;
            }
        }

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
    use crate::utils::message_bus::{MessageBus, MessageBusError};
    use crate::utils::state_machine::{StateMachine, StateMachineError};
    use crate::utils::thread_manager::ThreadManager;

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
    fn test_router_add_connection() -> Result<(), MessageBusError> {
        let mut message_bus: MessageBus = MessageBus::new();
        let channel_id = message_bus.create_channel(0)?;
        let receiver = message_bus.subscribe(channel_id)?;
        let connection = RouterConnection {
            channel: RouterChannel::Inbound(receiver),
            filters: Vec::new(),
        };
        let mut router = Router::new(Uuid::new_v4());

        router.add_connection(connection);
        assert!(router.connections.len() == 1);
        Ok(())
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
    fn test_router_state_work() -> Result<(), StateMachineError> {
        use crate::components::advertisement::{AdvertisementType, Announcement};
        use crate::components::route::PathElement;
        use crate::utils::message_bus::Message;
        use ip_network::IpNetwork;
        use std::net::IpAddr;
        use std::net::Ipv4Addr;

        let mut thread_manager: ThreadManager = ThreadManager::new();
        let router_id = Uuid::new_v4();
        let mut router = Router::new(router_id.clone());
        let channel_id = Uuid::new_v4();

        // Create and set up BGP RIB
        let bgp_rib = Arc::new(Mutex::new(BgpRib::new()));
        router.set_rib(&bgp_rib);

        if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
            message_bus.create_channel_with_uuid(1, channel_id)?;
            let receiver: std::sync::mpsc::Receiver<Box<dyn Message + 'static>> =
                message_bus.subscribe(channel_id)?;
            let router_connection = RouterConnection {
                channel: RouterChannel::Inbound(receiver),
                filters: Vec::new(),
            };
            router.add_connection(router_connection);
        }

        let mut state_machine = StateMachine::new(&mut thread_manager, router)?;
        state_machine.start()?;

        if let Ok(message_bus) = thread_manager.lock_message_bus() {
            let sender = message_bus.publish(channel_id)?;
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
            };
            sender
                .send(Box::new(advertisement))
                .map_err(|e| StateMachineError::StateMachineError(e.to_string()))?;
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
        state_machine.stop()?;

        let rib = bgp_rib.lock().unwrap();
        assert!(rib.get_prefix_count() == (1, 0));
        let routes =
            rib.get_routes_for_router(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)), &router_id);
        assert!(routes.len() == 1);
        assert!(routes[0].prefix == IpNetwork::new(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap());
        assert!(routes[0].next_hop == "192.168.1.1");
        assert!(routes[0].as_path == vec![PathElement::ASN(1), PathElement::ASN(2)]);
        Ok(())
    }
}
