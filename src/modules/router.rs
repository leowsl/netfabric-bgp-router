use crate::components::advertisment::Advertisement;
use crate::components::bgp_rib::BgpRib;
use crate::components::ris_live_data::RisLiveData;
use crate::utils::message_bus::MessageReceiver;
use crate::utils::state_machine::{State, StateTransition};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct Router {
    pub id: Uuid,
    receivers: Vec<Arc<Mutex<MessageReceiver>>>,
    bgp_rib: Option<Arc<Mutex<BgpRib>>>,
}

impl Router {
    pub fn new(id: Uuid) -> Self {
        Router {
            id,
            receivers: Vec::new(),
            bgp_rib: None,
        }
    }

    pub fn add_receiver(&mut self, receiver: MessageReceiver) {
        self.receivers.push(Arc::new(Mutex::new(receiver)));
    }

    pub fn set_rib(&mut self, rib: &Arc<Mutex<BgpRib>>) {
        self.bgp_rib = Some(rib.clone());
    }
}

impl State for Router {
    fn work(&mut self) -> StateTransition {
        for receiver in &self.receivers {
            if let Ok(guard) = receiver.lock() {
                if let Ok(msg) = guard.try_recv() {
                    if let Some(bgp_msg) = msg.cast::<RisLiveData>() {
                        if let Some(rib) = &self.bgp_rib {
                            if let Ok(mut rib) = rib.lock() {
                                let updates = Advertisement::from(bgp_msg).get_updates();
                                for update in updates {
                                    rib.update_route(update);
                                }
                            }
                        }
                    }
                }
            }
        }
        StateTransition::Continue
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
    fn test_router_add_receiver() -> Result<(), MessageBusError> {
        let mut message_bus: MessageBus = MessageBus::new();
        let channel_id = message_bus.create_channel(0)?;
        let receiver = message_bus.subscribe(channel_id)?;
        let mut router = Router::new(Uuid::new_v4());

        router.add_receiver(receiver);
        assert!(router.receivers.len() == 1);
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

        if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
            message_bus.create_channel_with_uuid(0, channel_id)?;
            let receiver = message_bus.subscribe(channel_id)?;
            router.add_receiver(receiver);
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
