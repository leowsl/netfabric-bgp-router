use crate::components::bgp::bgp_config::SessionConfig;
use crate::components::bgp::bgp_process::BgpProcess;
use crate::components::bgp::bgp_rib::BgpRib;
use crate::components::bgp::bgp_session::{BgpSession, BgpSessionError};
use crate::components::interface::Interface;
use crate::modules::router::{Router, RouterError};
use crate::utils::message_bus::MessageBusError;
use crate::utils::state_machine::{StateMachine, StateMachineError};
use crate::utils::thread_manager::{ThreadManager, ThreadManagerError};
use crate::utils::topology::NetworkTopology;
use log::info;
use std::collections::HashMap;
use std::net::IpAddr;
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

    pub fn from_topology(
        topology: &NetworkTopology,
        thread_manager: &'a mut ThreadManager,
    ) -> Result<Self, NetworkManagerError> {
        let mut network = NetworkManager::new(thread_manager);

        // Create routers
        for (vertex_id, vertex) in topology.get_topology().iter_vertices() {
            let process = BgpProcess::from_config(vertex.process_config.clone());
            let router = Router::new(vertex_id.clone())
                .with_options(vertex.router_options.clone())
                .with_bgp_process(process);
            network.insert_router(router);
        }

        // Create sessions between routers
        for (vertex1_id, vertex2_id, edge) in topology.get_topology().iter_edges() {
            let (session1, session2) = network.new_bgp_session_pair(
                edge.session_configs.0.clone(),
                edge.session_configs.1.clone(),
                edge.link_buffer_size as usize,
            )?;
            let (router1, router2) = network.get_router_pair_mut(&vertex1_id, &vertex2_id)?;

            router1
                .get_bgp_processes_mut()
                .get_mut(0)
                .ok_or_else(|| {
                    RouterError::ProcessError(
                        "Router has more or less than one BGP process".to_string(),
                    )
                })
                .and_then(|process| Ok(process.add_session(session1)))?;

            router2
                .get_bgp_processes_mut()
                .get_mut(0)
                .ok_or_else(|| {
                    RouterError::ProcessError(
                        "Router has more or less than one BGP process".to_string(),
                    )
                })
                .and_then(|process| Ok(process.add_session(session2)))?;
        }
        Ok(network)
    }

    pub fn borrow_thread_manager(&'a mut self) -> &'a mut ThreadManager {
        self.thread_manager
    }

    pub fn get_rib_mutex(&self) -> Arc<Mutex<BgpRib>> {
        self.rib.clone()
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
            for process in router.get_bgp_processes_mut() {
                // TODO: find out why the there is a deadlock with the shared rib mutes!

                // process.set_rib(self.rib.clone());
                process.set_router_id(&id);
            }
            self.routers.insert(id, router);
        }
    }

    pub fn get_router_mut(&mut self, id: &Uuid) -> Option<&mut Router> {
        self.routers.get_mut(id)
    }

    pub fn get_router_pair_mut(
        &mut self,
        router1_id: &Uuid,
        router2_id: &Uuid,
    ) -> Result<(&mut Router, &mut Router), NetworkManagerError> {
        if !self.routers.contains_key(router1_id) {
            return Err(NetworkManagerError::RouterNotFound(*router1_id));
        }
        if !self.routers.contains_key(router2_id) {
            return Err(NetworkManagerError::RouterNotFound(*router2_id));
        }

        let [Some(router1), Some(router2)] =
            self.routers.get_disjoint_mut([router1_id, router2_id])
        else {
            panic!("Couldn't get routers, even though the ids are keys in the HashMap");
        };
        Ok((router1, router2))
    }

    pub fn new_interface_pair(
        &self,
        ip1: IpAddr,
        ip2: IpAddr,
        link_buffer_size: usize,
    ) -> Result<(Interface, Interface), NetworkManagerError> {
        let mut interface1 = Interface::new(ip1);
        let mut interface2 = Interface::new(ip2);

        let (tx1, rx1) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;

        let (tx2, rx2) = self
            .thread_manager
            .get_message_bus_channel_pair(link_buffer_size)?;

        interface1.set_out_channel(tx1);
        interface2.set_in_channel(rx1);
        interface2.set_out_channel(tx2);
        interface1.set_in_channel(rx2);

        Ok((interface1, interface2))
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

    pub fn new_bgp_session_pair(
        &self,
        config1: SessionConfig,
        config2: SessionConfig,
        link_buffer_size: usize,
    ) -> Result<(BgpSession, BgpSession), NetworkManagerError> {
        let (interface1, interface2) = self.new_interface_pair(
            config1.interface_ip.clone(),
            config2.interface_ip.clone(),
            link_buffer_size,
        )?;

        let session1 = BgpSession::from_config(config1).with_interface(interface1);
        let session2 = BgpSession::from_config(config2).with_interface(interface2);

        Ok((session1, session2))
    }

    pub fn connect_bgp_session_pair(
        &mut self,
        session1: &mut BgpSession,
        session2: &mut BgpSession,
        link_buffer_size: usize,
    ) -> Result<(), NetworkManagerError> {
        if session1.get_interface().is_some() || session2.get_interface().is_some() {
            return Err(NetworkManagerError::ConnectionError(
                "BGP session already connected".to_string(),
            ));
        }
        let (interface1, interface2) = self.new_interface_pair(
            session1.get_config().interface_ip.clone(),
            session2.get_config().interface_ip.clone(),
            link_buffer_size,
        )?;
        session1.set_interface(interface1);
        session2.set_interface(interface2);
        Ok(())
    }

    pub fn new_router_with_bgp_process(
        &mut self,
        mut process: BgpProcess,
    ) -> Result<Uuid, NetworkManagerError> {
        let router_id = Uuid::new_v4();
        process.set_router_id(&router_id);
        self.insert_router(Router::new(router_id).with_bgp_process(process));
        Ok(router_id)
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
    BgpSessionError(BgpSessionError),
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

impl From<BgpSessionError> for NetworkManagerError {
    fn from(error: BgpSessionError) -> Self {
        NetworkManagerError::BgpSessionError(error)
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
    use crate::components::advertisement::{Advertisement, Announcement};
    use crate::components::bgp::bgp_config::ProcessConfig;
    use crate::components::route::PathElement;
    use crate::modules::router::{Router, RouterOptions};
    use std::net::Ipv4Addr;

    /// Helper function to test connectivity between two BgpSession instances.
    fn assert_bgp_sessions_connectivity(
        session1: &mut BgpSession,
        session2: &mut BgpSession,
    ) -> Result<(), NetworkManagerError> {
        let ad = Advertisement::default();
        session1.send(vec![ad.clone(); 5])?;
        session2.send(vec![ad.clone(); 5])?;

        let rec1 = session1.receive()?;
        let rec2 = session2.receive()?;
        assert_eq!(rec1.len(), 5);
        assert_eq!(rec2.len(), 5);
        for i in 0..5 {
            assert_eq!(
                rec1[i], ad,
                "Session1 received wrong advertisement at index {i}: {:?}",
                rec1[i]
            );
            assert_eq!(
                rec2[i], ad,
                "Session2 received wrong advertisement at index {i}: {:?}",
                rec2[i]
            );
        }
        Ok(())
    }

    /// Helper function to test connectivity between two BgpProcess instances.
    fn assert_bgp_processes_connectivity(
        process1: &mut BgpProcess,
        process2: &mut BgpProcess,
        test_with_rib: bool,
    ) -> Result<(), NetworkManagerError> {
        let ad1 = Advertisement {
            timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
            peer: "192.168.0.1".to_string(),
            path: Some(vec![PathElement::ASN(1)]),
            announcements: Some(vec![Announcement {
                prefixes: vec!["1.0.0.0/8".to_string()],
                next_hop: "192.168.0.1".to_string(),
            }]),
            ..Default::default()
        };
        let ad2 = Advertisement {
            timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
            peer: "192.168.0.2".to_string(),
            path: Some(vec![PathElement::ASN(2)]),
            announcements: Some(vec![Announcement {
                prefixes: vec!["2.0.0.0/8".to_string()],
                next_hop: "192.168.0.2".to_string(),
            }]),
            ..Default::default()
        };
        // Clear previous iteration's advertisements
        process1.clear_advertisements();
        process1.clear_bestroute_updates();
        process2.clear_advertisements();
        process2.clear_bestroute_updates();
        process1.rib_interface.clear_rib();
        process2.rib_interface.clear_rib();

        process1.inject_advertisement(ad1.clone());
        process2.inject_advertisement(ad2.clone());

        process1.send();
        process2.send();

        process1.receive();
        process2.receive();

        if test_with_rib {
            let bestroute1 = process1
                .rib_interface
                .get_bestroute(IpAddr::V4(Ipv4Addr::new(2, 0, 10, 25)))
                .unwrap();
            assert_eq!(bestroute1.0.next_hop, "192.168.0.2".to_string());

            let bestroute2 = process2
                .rib_interface
                .get_bestroute(IpAddr::V4(Ipv4Addr::new(1, 0, 10, 25)))
                .unwrap();
            assert_eq!(bestroute2.0.next_hop, "192.168.0.1".to_string());

            process1.get_best_route_changes();
            process2.get_best_route_changes();

            process1.generate_advertisement();
            process2.generate_advertisement();

            let p1_ad = process1.get_advertisements().get(0).unwrap();
            assert_eq!(p1_ad.peer, process1.config.ip.to_string());
            assert_eq!(p1_ad.announcements, ad2.announcements);
            assert_eq!(p1_ad.id, process1.id.to_string());

            let p2_ad = process2.get_advertisements().get(0).unwrap();
            assert_eq!(p2_ad.peer, process2.config.ip.to_string());
            assert_eq!(p2_ad.announcements, ad1.announcements);
            assert_eq!(p2_ad.id, process2.id.to_string());
        }
        Ok(())
    }

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

        // TODO: Fix this test with new functions
        // assert!(matches!(
        //     network.create_router_interface_pair(
        //         (&router_id, &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
        //         (&Uuid::new_v4(), &IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
        //         10,
        //         10,
        //     ),
        //     Err(NetworkManagerError::RouterNotFound(_))
        // ));
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
    fn new_interface_pair() {
        let mut thread_manager = ThreadManager::new();
        let network = NetworkManager::new(&mut thread_manager);

        let result = network.new_interface_pair(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)),
            100,
        );
        assert!(result.is_ok());

        let (interface1, interface2) = result.unwrap();
        assert_eq!(
            interface1.get_ip_address(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
        );
        assert_eq!(
            interface2.get_ip_address(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))
        );
    }

    #[test]
    fn get_router_pair_mut() {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let router1_id = Uuid::new_v4();
        let router2_id = Uuid::new_v4();
        network.create_router(router1_id);
        network.create_router(router2_id);

        let result = network.get_router_pair_mut(&router1_id, &router2_id);
        assert!(result.is_ok());
        let (router1, router2) = result.unwrap();
        assert_eq!(router1.id, router1_id);
        assert_eq!(router2.id, router2_id);
    }

    #[test]
    fn new_bgp_session_pair() -> Result<(), NetworkManagerError> {
        let mut thread_manager = ThreadManager::new();
        let network = NetworkManager::new(&mut thread_manager);

        let config1 = SessionConfig {
            session_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            interface_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            ..Default::default()
        };
        let config2 = SessionConfig {
            session_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            interface_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)),
            ..Default::default()
        };

        // Test Session Creation
        let result = network.new_bgp_session_pair(config1, config2, 100);
        assert!(result.is_ok());

        let (mut session1, mut session2) = result.unwrap();
        assert_eq!(
            session1.get_interface().unwrap().get_ip_address(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
        );
        assert_eq!(
            session2.get_interface().unwrap().get_ip_address(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))
        );

        // Test Session Connectivity
        assert_bgp_sessions_connectivity(&mut session1, &mut session2)?;

        Ok(())
    }

    #[test]
    fn connect_bgp_session_pair() -> Result<(), NetworkManagerError> {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let mut session1 = BgpSession::from_config(SessionConfig {
            session_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            interface_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            ..Default::default()
        });
        let mut session2 = BgpSession::from_config(SessionConfig {
            session_ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            interface_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)),
            ..Default::default()
        });

        // Test Session Creation
        let result = network.connect_bgp_session_pair(&mut session1, &mut session2, 100);
        assert!(result.is_ok());

        assert_eq!(
            session1.get_interface().unwrap().get_ip_address(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
        );
        assert_eq!(
            session2.get_interface().unwrap().get_ip_address(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))
        );

        // Test Session Connectivity
        assert_bgp_sessions_connectivity(&mut session1, &mut session2)?;

        Ok(())
    }

    #[test]
    fn new_router_with_bgp_process() -> Result<(), NetworkManagerError> {
        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let process = BgpProcess::from_config(ProcessConfig {
            as_number: 65000,
            ..Default::default()
        });
        let router_id = network.new_router_with_bgp_process(process)?;

        let router = network.get_router_mut(&router_id).unwrap();
        let bgp_processes = router.get_bgp_processes();
        assert_eq!(bgp_processes.len(), 1);
        assert_eq!(bgp_processes[0].config.as_number, 65000);
        Ok(())
    }

    #[test]
    fn test_from_topology() {
        let mut thread_manager = ThreadManager::new();
        let mut topology = NetworkTopology::new();

        // Create Triangle Topology + One single router
        let r1 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
        let r2 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
        let r3 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
        let r4 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
        topology.add_connection(
            r1,
            r2,
            (SessionConfig::default(), SessionConfig::default()),
            100,
        );
        topology.add_connection(
            r2,
            r3,
            (SessionConfig::default(), SessionConfig::default()),
            100,
        );
        topology.add_connection(
            r1,
            r3,
            (SessionConfig::default(), SessionConfig::default()),
            100,
        );
        let mut network = NetworkManager::from_topology(&topology, &mut thread_manager).unwrap();

        // Check if the network is created correctly
        let routers = &network.routers;
        assert!(routers.contains_key(&r1));
        assert!(routers.contains_key(&r2));
        assert!(routers.contains_key(&r3));
        assert!(routers.contains_key(&r4));

        let (router1, router2) = network.get_router_pair_mut(&r1, &r2).unwrap();
        assert_bgp_processes_connectivity(
            router1.get_bgp_processes_mut().get_mut(0).unwrap(),
            router2.get_bgp_processes_mut().get_mut(0).unwrap(),
            true,
        )
        .unwrap();

        let (router1, router3) = network.get_router_pair_mut(&r1, &r2).unwrap();
        assert_bgp_processes_connectivity(
            router1.get_bgp_processes_mut().get_mut(0).unwrap(),
            router3.get_bgp_processes_mut().get_mut(0).unwrap(),
            true,
        )
        .unwrap();

        let (router2, router3) = network.get_router_pair_mut(&r1, &r2).unwrap();
        assert_bgp_processes_connectivity(
            router2.get_bgp_processes_mut().get_mut(0).unwrap(),
            router3.get_bgp_processes_mut().get_mut(0).unwrap(),
            true,
        )
        .unwrap();

        network.start().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        network.stop().unwrap();
    }
}
