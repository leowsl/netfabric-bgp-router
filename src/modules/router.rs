use crate::components::advertisment::Advertisement;
use crate::components::bgp_rib::BgpRib;
use crate::utils::message_bus::MessageBusError;
use crate::utils::message_bus::{MessageReceiver, MessageSender};
use crate::utils::state_machine::{State, StateTransition};
use log::warn;
use std::sync::{Arc, Mutex, PoisonError, TryLockError};
use thiserror::Error;
use uuid::Uuid;

pub enum RouterChannel {
    Inbound(MessageReceiver),
    Outbound(MessageSender),
}
pub trait Filter<T>: 'static + Send + Sync {
    fn filter(&self, route: &T) -> bool;
}
pub struct RouterConnection {
    pub channel: RouterChannel,
    pub filters: Vec<Box<dyn Filter<Advertisement>>>,
}

pub struct Router {
    pub id: Uuid,
    connections: Vec<Arc<Mutex<RouterConnection>>>,
    bgp_rib: Option<Arc<Mutex<BgpRib>>>,
}

impl Router {
    pub fn new(id: Uuid) -> Self {
        Router {
            id,
            connections: Vec::new(),
            bgp_rib: None,
        }
    }

    pub fn add_connection(&mut self, connection: RouterConnection) {
        self.connections.push(Arc::new(Mutex::new(connection)));
    }

    pub fn set_rib(&mut self, rib: &Arc<Mutex<BgpRib>>) {
        self.bgp_rib = Some(rib.clone());
    }

    pub fn process_incoming_advertisements(&mut self) -> Result<(), RouterError> {
        let rib_mutex = self
            .bgp_rib
            .as_ref()
            .ok_or_else(|| RouterError::RibNotSet(self.id.to_string()))?;
        for connection_mutex in &self.connections {
            match &connection_mutex.try_lock()?.channel {
                RouterChannel::Outbound(_) => continue,
                RouterChannel::Inbound(receiver) => {
                    while let Ok(msg) = receiver.try_recv() {
                        if let Some(advertisement) = msg.cast::<Advertisement>() {
                            rib_mutex
                                .try_lock()?
                                .update_routes(advertisement.get_routes());
                                //TODO apply filters
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn process_outgoing_advertisements(&mut self) -> Result<(), RouterError> {
        // TODO: Implement this
        Ok(())
    }
}
impl State for Router {
    fn work(&mut self) -> StateTransition {
        // Process incoming advertisements
        match self.process_incoming_advertisements() {
            Ok(_) => (),
            Err(e) => {
                warn!("Error processing incoming advertisements: {}", e);
                return StateTransition::Continue;
            }
        }

        // Process outgoing advertisements
        match self.process_outgoing_advertisements() {
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
        use crate::components::advertisment::AdvertisementType;
        use crate::components::ris_live_data::{RisLiveData, RisLiveMessage};

        let mut thread_manager: ThreadManager = ThreadManager::new();
        let mut router = Router::new(Uuid::new_v4());
        let channel_id = Uuid::new_v4();

        // Create and set up BGP RIB
        let bgp_rib = Arc::new(Mutex::new(BgpRib::new()));
        router.set_rib(&bgp_rib);

        if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
            message_bus.create_channel_with_uuid(0, channel_id)?;
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
            sender
                .send(Box::new(RisLiveMessage {
                    msg_type: "RisLiveMessage".to_string(),
                    data: RisLiveData {
                        timestamp: 0.0,
                        peer: "192.168.1.1".to_string(),
                        peer_asn: "1".to_string(),
                        id: "test".to_string(),
                        host: "test".to_string(),
                        msg_type: AdvertisementType::Update,
                        path: None,
                        community: None,
                        origin: None,
                        announcements: None,
                        raw: None,
                        withdrawals: None,
                        aggregator: None,
                        asn: None,
                        capabilities: None,
                        med: None,
                        direction: None,
                        version: None,
                        hold_time: None,
                        router_id: None,
                        notification: None,
                        state: None,
                    },
                }))
                .map_err(|e| StateMachineError::StateMachineError(e.to_string()))?;
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
        state_machine.stop()?;
        Ok(())
    }
}
