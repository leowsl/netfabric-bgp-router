use crate::components::bgp::bgp_rib::BgpRib;
use crate::components::filters::Filter;
use crate::components::interface::Interface;
use crate::modules::router::Router;
use crate::utils::message_bus::MessageBusError;
use crate::utils::state_machine::{StateMachine, StateMachineError};
use crate::utils::thread_manager::{ThreadManager, ThreadManagerError};
use log::info;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::router::RouterError;

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

    pub fn connect_interface_pair(
        &mut self,
        interface1: &mut Interface,
        interface2: &mut Interface,
        link_buffer_size: usize,
    ) -> Result<(), NetworkManagerError> {
        let (tx, rx) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;
        interface1.set_out_channel(tx);
        interface2.set_in_channel(rx);
        Ok(())
    }

    pub fn insert_router_interface(
        &mut self,
        router_id: &Uuid,
        interface: Interface,
    ) -> Result<(), NetworkManagerError> {
        let router = self
            .routers
            .get_mut(router_id)
            .ok_or(NetworkManagerError::RouterNotFound(*router_id))?;
        router.add_interface(interface);
        Ok(())
    }

    pub fn create_router_interface_pair(
        &mut self,
        (router_id, router_ip): (&Uuid, &IpAddr),
        (peer_id, peer_ip): (&Uuid, &IpAddr),
        link_buffer_size: usize,
        interface_buffer_size: usize,
    ) -> Result<(Uuid, Uuid), NetworkManagerError> {
        if !self.routers.contains_key(router_id) {
            return Err(NetworkManagerError::RouterNotFound(*router_id));
        }
        if !self.routers.contains_key(peer_id) {
            return Err(NetworkManagerError::RouterNotFound(*peer_id));
        }

        let mut router_interface = Interface::new(router_ip.clone());
        let mut peer_interface = Interface::new(peer_ip.clone());

        router_interface.set_buffer_size(interface_buffer_size, interface_buffer_size);
        peer_interface.set_buffer_size(interface_buffer_size, interface_buffer_size);

        let router_interface_id = router_interface.id;
        let peer_interface_id = peer_interface.id;

        let (tx, rx) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;

        router_interface.set_out_channel(tx);
        peer_interface.set_in_channel(rx);

        self.insert_router_interface(router_id, router_interface)?;
        self.insert_router_interface(peer_id, peer_interface)?;

        Ok((router_interface_id, peer_interface_id))
    }

    pub fn create_router_interface_pair_duplex(
        &mut self,
        (router_id, router_ip): (&Uuid, &IpAddr),
        (peer_id, peer_ip): (&Uuid, &IpAddr),
        link_buffer_size: usize,
        interface_buffer_size: usize,
    ) -> Result<(Uuid, Uuid), NetworkManagerError> {
        let mut router_interface = Interface::new(router_ip.clone());
        let mut peer_interface = Interface::new(peer_ip.clone());

        router_interface.set_buffer_size(interface_buffer_size, interface_buffer_size);
        peer_interface.set_buffer_size(interface_buffer_size, interface_buffer_size);

        let router_interface_id = router_interface.id;
        let peer_interface_id = peer_interface.id;

        let (tx1, rx1) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;
        let (tx2, rx2) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;

        router_interface.set_in_channel(rx1);
        router_interface.set_out_channel(tx2);
        peer_interface.set_out_channel(tx1);
        peer_interface.set_in_channel(rx2);

        self.insert_router_interface(router_id, router_interface)?;
        self.insert_router_interface(peer_id, peer_interface)?;

        Ok((router_interface_id, peer_interface_id))
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
    RouterError(RouterError),
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

impl From<RouterError> for NetworkManagerError {
    fn from(error: RouterError) -> Self {
        NetworkManagerError::RouterError(error)
    }
}

impl std::fmt::Display for NetworkManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl std::error::Error for NetworkManagerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::advertisement::Advertisement;
    use crate::modules::router::{Router, RouterOptions};
    use std::net::Ipv4Addr;

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

        assert!(network.get_router_mut(&router_id).is_none());

        assert!(matches!(
            network.create_router_interface_pair(
                (&router_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
                (&Uuid::new_v4(), &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
                10,
                10,
            ),
            Err(NetworkManagerError::RouterNotFound(_))
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
            .create_router_interface_pair(
                (&router1_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
                (&router2_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
                10,
                10,
            )
            .unwrap();
        network
            .create_router_interface_pair_duplex(
                (&router1_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
                (&router3_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 3))),
                10,
                10,
            )
            .unwrap();

        assert!(network.start().is_ok());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(network.stop().is_ok());
    }

    #[test]
    fn insert_router_interface() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let router_id = Uuid::new_v4();
        network.create_router(router_id);

        let interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        network
            .insert_router_interface(&router_id, interface)
            .unwrap();

        assert!(network.start().is_ok());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(network
            .router_sm
            .get(&router_id)
            .unwrap()
            .get_runner_active(&network.thread_manager));
        assert!(network.stop().is_ok());
    }

    #[test]
    fn create_router_interface_pair() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let router1_id = Uuid::new_v4();
        let router2_id = Uuid::new_v4();
        network.create_router(router1_id);
        network.create_router(router2_id);

        // No Duplex
        network
            .create_router_interface_pair(
                (&router1_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
                (&router2_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
                10,
                10,
            )
            .unwrap();

        // Duplex
        network
            .create_router_interface_pair_duplex(
                (&router1_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
                (&router2_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
                10,
                10,
            )
            .unwrap();

        assert!(network.start().is_ok());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(network
            .router_sm
            .get(&router1_id)
            .unwrap()
            .get_runner_active(&network.thread_manager));
        assert!(network
            .router_sm
            .get(&router2_id)
            .unwrap()
            .get_runner_active(&network.thread_manager));
        assert!(network.stop().is_ok());
    }

    #[test]
    fn router_interface_send_receive() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        // Create two routers and connect them
        let router1_id = Uuid::new_v4();
        let router2_id = Uuid::new_v4();

        network.create_router(router1_id);
        network.create_router(router2_id);

        // At the moment, packets are infinetely looped, so we increase capacity as a hack
        let router1 = network.get_router_mut(&router1_id).unwrap();
        router1.set_options(RouterOptions {
            capacity: 10000,
            ..Default::default()
        });
        let router2 = network.get_router_mut(&router2_id).unwrap();
        router2.set_options(RouterOptions {
            capacity: 10000,
            ..Default::default()
        });

        network
            .create_router_interface_pair_duplex(
                (&router1_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
                (&router2_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
                10,
                10,
            )
            .unwrap();

        // Create duplex external connections to router 1
        let (tx1, router1_rx) = network
            .thread_manager
            .get_message_bus_channel_pair(100)
            .unwrap();
        let (router1_tx, rx1) = network
            .thread_manager
            .get_message_bus_channel_pair(10)
            .unwrap();
        let mut interface1 = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        interface1.set_out_channel(tx1);
        interface1.set_in_channel(rx1);
        network
            .insert_router_interface(&router1_id, interface1)
            .unwrap();

        // Create duplex external connections to router 2
        let (tx2, router2_rx) = network
            .thread_manager
            .get_message_bus_channel_pair(100)
            .unwrap();
        let (router2_tx, rx2) = network
            .thread_manager
            .get_message_bus_channel_pair(100)
            .unwrap();
        let mut interface2 = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)));
        interface2.set_out_channel(tx2);
        interface2.set_in_channel(rx2);
        network
            .insert_router_interface(&router2_id, interface2)
            .unwrap();

        // Start network
        assert!(network.start().is_ok());
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Send
        let test_ad = Advertisement {
            timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
            peer: "192.168.1.2".to_string(),
            ..Default::default()
        };
        for _ in 0..5 {
            router1_tx.send(Box::new(test_ad.clone())).unwrap();
            router2_tx.send(Box::new(test_ad.clone())).unwrap();
        }

        for _ in 0..5 {
            let msg = router1_rx.recv().unwrap();
            let received_ad = msg.cast::<Advertisement>().unwrap();
            assert_eq!(received_ad, &test_ad);

            let msg = router2_rx.recv().unwrap();
            let received_ad = msg.cast::<Advertisement>().unwrap();
            assert_eq!(received_ad, &test_ad);
        }

        assert!(network.stop().is_ok());
    }
}
