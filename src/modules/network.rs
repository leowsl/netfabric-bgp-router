use crate::components::advertisement::Advertisement;
use crate::components::bgp_rib::BgpRib;
use crate::components::filters::{Filter, NoFilter};
use crate::modules::router::{Router, RouterChannel, RouterConnection};
use crate::utils::message_bus::{MessageBusError, MessageReceiver, MessageSender};
use crate::utils::state_machine::{StateMachine, StateMachineError};
use crate::utils::thread_manager::{ThreadManager, ThreadManagerError};
use log::info;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct NetworkManager<'a> {
    rib: Arc<Mutex<BgpRib>>,
    routers: HashMap<Uuid, Router>,
    router_sm: HashMap<Uuid, StateMachine>,
    thread_manager: &'a mut ThreadManager,
}

impl<'a> NetworkManager<'a> {
    pub fn new(thread_manager: &'a mut ThreadManager) -> Self {
        NetworkManager {
            rib: Arc::new(Mutex::new(BgpRib::new())),
            routers: HashMap::new(),
            router_sm: HashMap::new(),
            thread_manager: thread_manager,
        }
    }

    pub fn get_rib_lock(&self) -> std::sync::MutexGuard<'_, BgpRib> {
        self.rib.lock().unwrap()
    }

    pub fn get_rib_clone(&self) -> BgpRib {
        self.rib.lock().unwrap().clone()
    }

    pub fn create_router(&mut self, id: Uuid) {
        self.insert_router(Router::new(id));
    }

    pub fn insert_router(&mut self, mut router: Router) {
        if self.routers.contains_key(&router.id) {
            panic!("Router already exists");
        } else {
            let id: Uuid = router.id.clone();
            router.set_rib(&self.rib);
            self.routers.insert(id, router);
        }
    }

    pub fn get_router_mut(&mut self, id: &Uuid) -> Option<&mut Router> {
        self.routers.get_mut(id)
    }

    pub fn connect_router_pair<F1: Filter<Advertisement>, F2: Filter<Advertisement>>(
        &mut self,
        router_id: &Uuid,
        peer_id: &Uuid,
        link_buffer_size: usize,
        filters: (F1, F2),
    ) -> Result<(), NetworkManagerError> {
        let [Some(router), Some(peer)] = self.routers.get_disjoint_mut([router_id, peer_id]) else {
            return Err(NetworkManagerError::ConnectionError(format!(
                "Router not found: {} {}",
                router_id, peer_id
            )));
        };

        let (tx, rx) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;

        router.add_connection(RouterConnection {
            channel: RouterChannel::Outbound(tx),
            filter: Box::new(filters.0),
        });
        peer.add_connection(RouterConnection {
            channel: RouterChannel::Inbound(rx),
            filter: Box::new(filters.1),
        });
        Ok(())
    }

    pub fn create_router_rx<F: Filter<Advertisement>>(
        &mut self,
        router_id: &Uuid,
        link_buffer_size: usize,
        filter: F,
    ) -> Result<MessageReceiver, NetworkManagerError> {
        let router = self
            .routers
            .get_mut(router_id)
            .ok_or(NetworkManagerError::RouterNotFound(*router_id))?;
        let (tx, rx) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;
        router.add_connection(RouterConnection {
            channel: RouterChannel::Outbound(tx),
            filter: Box::new(filter),
        });
        Ok(rx)
    }

    pub fn create_router_tx<F: Filter<Advertisement>>(
        &mut self,
        router_id: &Uuid,
        link_buffer_size: usize,
        filter: F,
    ) -> Result<MessageSender, NetworkManagerError> {
        let router = self
            .routers
            .get_mut(router_id)
            .ok_or(NetworkManagerError::RouterNotFound(*router_id))?;
        let (tx, rx) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;
        router.add_connection(RouterConnection {
            channel: RouterChannel::Inbound(rx),
            filter: Box::new(filter),
        });
        Ok(tx)
    }

    pub fn start(&mut self) -> Result<(), NetworkManagerError> {
        info!("Starting network");

        // Convert routers to state machines
        for (id, router) in self.routers.drain() {
            self.router_sm
                .insert(id, StateMachine::new(self.thread_manager, router)?);
        }

        // Start state machines
        for state_machine in self.router_sm.values_mut() {
            state_machine.start()?;
        }
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), NetworkManagerError> {
        info!("Stopping network");
        for state_machine in self.router_sm.values_mut() {
            state_machine.stop()?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum NetworkManagerError {
    RouterNotFound(Uuid),
    RouterAlreadyExists(Uuid),
    ConnectionError(String),
    StateMachineError(StateMachineError),
    MessageBusError(MessageBusError),
    ThreadManagerError(ThreadManagerError),
}

impl From<StateMachineError> for NetworkManagerError {
    fn from(error: StateMachineError) -> Self {
        NetworkManagerError::StateMachineError(error)
    }
}

impl From<MessageBusError> for NetworkManagerError {
    fn from(error: MessageBusError) -> Self {
        NetworkManagerError::MessageBusError(error)
    }
}

impl From<ThreadManagerError> for NetworkManagerError {
    fn from(error: ThreadManagerError) -> Self {
        NetworkManagerError::ThreadManagerError(error)
    }
}

impl std::fmt::Display for NetworkManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkManagerError::RouterNotFound(id) => write!(f, "Router not found: {}", id),
            NetworkManagerError::RouterAlreadyExists(id) => {
                write!(f, "Router already exists: {}", id)
            }
            NetworkManagerError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            NetworkManagerError::StateMachineError(e) => write!(f, "State machine error: {}", e),
            NetworkManagerError::MessageBusError(e) => write!(f, "Message bus error: {}", e),
            NetworkManagerError::ThreadManagerError(e) => write!(f, "Thread manager error: {}", e),
        }
    }
}

impl std::error::Error for NetworkManagerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::router::Router;

    #[test]
    fn test_create_network_manager() -> Result<(), NetworkManagerError> {
        let mut thread_manager = ThreadManager::new();
        let network = NetworkManager::new(&mut thread_manager);
        let rib = network.get_rib_lock();

        assert!(rib.get_prefix_count() == (0, 0));
        assert!(network.routers.is_empty());
        assert!(network.router_sm.is_empty());
        Ok(())
    }

    #[test]
    fn test_create_routers() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let router1_id = Uuid::new_v4();
        let router2_id = Uuid::new_v4();

        network.create_router(router1_id);
        network.insert_router(Router::new(router2_id));

        assert!(network.routers.contains_key(&router1_id));
        assert!(network.routers.contains_key(&router2_id));

        assert!(network.start().is_ok());
        assert!(network.router_sm.contains_key(&router1_id));
        assert!(network.router_sm.contains_key(&router2_id));

        assert!(network.stop().is_ok());
        std::thread::sleep(std::time::Duration::from_millis(100));

        assert!(!&network
            .router_sm
            .get(&router1_id)
            .unwrap()
            .get_runner_active(&network.thread_manager));
        assert!(!&network
            .router_sm
            .get(&router2_id)
            .unwrap()
            .get_runner_active(&network.thread_manager));
    }

    #[test]
    fn test_connect_routers() {
        use crate::components::advertisement::{Advertisement, AdvertisementType};
        use crate::components::filters::NoFilter;
        use crate::components::route::PathElement;

        // Set up network with two routers
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let router1_id = Uuid::new_v4();
        let router2_id = Uuid::new_v4();
        network.create_router(router1_id);
        network.create_router(router2_id);

        // Connect routers with test channels
        network
            .connect_router_pair(&router1_id, &router2_id, 10, (NoFilter, NoFilter))
            .unwrap();

        let tx = network.create_router_tx(&router1_id, 10, NoFilter).unwrap();
        let rx = network.create_router_rx(&router2_id, 10, NoFilter).unwrap();

        // Create test advertisement
        let test_ad = Advertisement {
            timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
            peer: "192.168.1.1".to_string(),
            peer_asn: "1".to_string(),
            id: "1".to_string(),
            host: "192.168.1.1".to_string(),
            msg_type: AdvertisementType::Update,
            path: Some(vec![PathElement::ASN(1)]),
            community: None,
            origin: None,
            announcements: None,
            withdrawals: None,
            raw: None,
        };

        // Start network and verify message passing
        network.start().unwrap();
        tx.send(Box::new(test_ad.clone())).unwrap();

        let received = rx.recv().unwrap();
        let received_ad = received.cast::<Advertisement>().unwrap();
        assert_eq!(received_ad, &test_ad);
    }

    #[test]
    #[should_panic(expected = "Router already exists")]
    fn test_duplicate_router() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);
        let router_id = Uuid::new_v4();
        network.create_router(router_id);
        network.create_router(router_id);
    }

    #[test]
    fn test_nonexistent_router_operations() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);
        let router_id = Uuid::new_v4();

        assert!(matches!(network.get_router_mut(&router_id), None));

        assert!(matches!(
            network.connect_router_pair(&router_id, &Uuid::new_v4(), 10, (NoFilter, NoFilter)),
            Err(NetworkManagerError::ConnectionError(_))
        ));
    }

    #[test]
    fn test_rib_operations() {
        let mut thread_manager = ThreadManager::new();
        let network = NetworkManager::new(&mut thread_manager);

        let rib_lock = network.get_rib_lock();
        assert_eq!(rib_lock.get_prefix_count(), (0, 0));
        drop(rib_lock);
        
        let rib_clone = network.get_rib_clone();
        assert_eq!(rib_clone.get_prefix_count(), (0, 0));
    }

    #[test]
    fn test_multiple_router_connections() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let router1_id = Uuid::new_v4();
        let router2_id = Uuid::new_v4();
        let router3_id = Uuid::new_v4();

        network.create_router(router1_id);
        network.create_router(router2_id);
        network.create_router(router3_id);

        // Connect router1 to both router2 and router3
        network
            .connect_router_pair(&router1_id, &router2_id, 10, (NoFilter, NoFilter))
            .unwrap();
        network
            .connect_router_pair(&router1_id, &router3_id, 10, (NoFilter, NoFilter))
            .unwrap();

        assert!(network.start().is_ok());
        assert!(network.stop().is_ok());
    }
}
