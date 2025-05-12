use std::collections::HashMap;

use crate::components::bgp_rib::BgpRib;
use crate::modules::router::{Router, RouterChannel, RouterConnection};
use crate::utils::message_bus::{MessageReceiver, MessageSender, MessageBusError};
use crate::utils::state_machine::{StateMachine, StateMachineError};
use crate::utils::thread_manager::ThreadManager;
use log::info;
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

    pub fn get_rib_mutex(&self) -> std::sync::MutexGuard<'_, BgpRib> {
        self.rib.lock().unwrap()
    }
    pub fn get_rib(&self) -> BgpRib {
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

    pub fn connect_router_pair(
        &mut self,
        router_id: &Uuid,
        peer_id: &Uuid,
        duplex: bool,
        link_buffer_size: usize,
    ) -> Result<(), NetworkManagerError> {
        let [Some(router), Some(peer)] = self.routers.get_disjoint_mut([router_id, peer_id]) else {
            return Err(NetworkManagerError::ConnectionError(format!(
                "Router not found: {} {}",
                router_id, peer_id
            )));
        };

        let (tx, rx) = if let Ok(mut message_bus) = self.thread_manager.lock_message_bus() {
            let channel_id = message_bus.create_channel(link_buffer_size).unwrap();
            (
                message_bus.publish(channel_id).unwrap(),
                message_bus.subscribe(channel_id).unwrap(),
            )
        } else {
            panic!("Failed to lock message bus");
        };

        router.add_connection(RouterConnection {
            channel: RouterChannel::Outbound(tx),
            filters: Vec::new(),
        });
        peer.add_connection(RouterConnection {
            channel: RouterChannel::Inbound(rx),
            filters: Vec::new(),
        });

        return match duplex {
            true => self.connect_router_pair(peer_id, router_id, false, link_buffer_size),
            false => Ok(()),
        };
    }

    pub fn create_router_rx(
        &mut self,
        router_id: &Uuid,
        link_buffer_size: usize,
    ) -> Result<MessageReceiver, NetworkManagerError> {
        let router = self
            .routers
            .get_mut(router_id)
            .ok_or(NetworkManagerError::RouterNotFound(*router_id))?;
        let (tx, rx) = if let Ok(mut mb) = self.thread_manager.lock_message_bus() {
            let channel_id = mb.create_channel(link_buffer_size)?;
            (mb.publish(channel_id)?, mb.subscribe(channel_id)?)
        } else {
            panic!("Failed to lock message bus");
        };
        router.add_connection(RouterConnection {
            channel: RouterChannel::Outbound(tx),
            filters: Vec::new(),
        });
        Ok(rx)
    }

    pub fn create_router_tx(
        &mut self,
        router_id: &Uuid,
        link_buffer_size: usize,
    ) -> Result<MessageSender, NetworkManagerError> {
        let router = self
            .routers
            .get_mut(router_id)
            .ok_or(NetworkManagerError::RouterNotFound(*router_id))?;
        let (tx, rx) = if let Ok(mut mb) = self.thread_manager.lock_message_bus() {
            let channel_id = mb.create_channel(link_buffer_size)?;
            (mb.publish(channel_id)?, mb.subscribe(channel_id)?)
        } else {
            panic!("Failed to lock message bus");
        };
        router.add_connection(RouterConnection {
            channel: RouterChannel::Inbound(rx),
            filters: Vec::new(),
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
        }
    }
}

impl std::error::Error for NetworkManagerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_router_pair() {
        use crate::components::advertisement::{Advertisement, AdvertisementType};
        use crate::components::route::PathElement;

        let mut thread_manager = ThreadManager::new();
        let mut network = NetworkManager::new(&mut thread_manager);

        let r1 = Uuid::new_v4();
        let r2 = Uuid::new_v4();
        network.create_router(r1);
        network.create_router(r2);

        network
            .connect_router_pair(&r1, &r2, true, 10)
            .unwrap();

        let rx = network.create_router_rx(&r1, 10).unwrap();
        let tx = network.create_router_tx(&r2, 10).unwrap();

        let ad = Advertisement {
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

        network.start().unwrap();

        tx.send(Box::new(ad.clone())).unwrap();
        let rec_box = rx.recv().unwrap();
        let rec = rec_box.cast::<Advertisement>().unwrap();
        assert_eq!(rec, &ad);
    }
}
