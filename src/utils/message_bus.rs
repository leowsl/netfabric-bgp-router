use crate::utils::thread_manager::ThreadManagerError;
use std::any::Any;
use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum MessageBusError {
    #[error("Channel with id {0} already exists")]
    ChannelExists(Uuid),
    #[error("Channel with id {0} not found")]
    ChannelNotFound(Uuid),
    #[error("Thread manager error: {0}")]
    ThreadManagerError(#[from] ThreadManagerError),
    #[error("Send error: {0}")]
    SendError(#[from] std::sync::mpsc::SendError<Box<dyn Message>>),
}

pub trait Message: Any + Send + Sync + 'static {}
pub type MessageSender = Arc<SyncSender<Box<dyn Message>>>;
pub type MessageReceiver = Receiver<Box<dyn Message>>;

impl dyn Message {
    pub fn cast<T: 'static>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

pub struct MessageBus {
    senders: HashMap<Uuid, MessageSender>,
    receivers: HashMap<Uuid, MessageReceiver>,
}

impl MessageBus {
    pub fn new() -> Self {
        MessageBus {
            senders: HashMap::new(),
            receivers: HashMap::new(),
        }
    }

    pub fn create_channel(&mut self, bound: usize) -> Result<Uuid, MessageBusError> {
        return self.create_channel_with_uuid(bound, Uuid::new_v4());
    }

    pub fn create_channel_with_uuid(
        &mut self,
        bound: usize,
        uuid: Uuid,
    ) -> Result<Uuid, MessageBusError> {
        if self.senders.contains_key(&uuid) || self.receivers.contains_key(&uuid) {
            return Err(MessageBusError::ChannelExists(uuid));
        }
        let (tx, rx) = sync_channel::<Box<dyn Message>>(bound);
        self.senders.insert(uuid, Arc::new(tx));
        self.receivers.insert(uuid, rx);
        Ok(uuid)
    }

    pub fn subscribe(&mut self, id: Uuid) -> Result<MessageReceiver, MessageBusError> {
        self.receivers
            .remove(&id)
            .ok_or(MessageBusError::ChannelNotFound(id))
    }

    pub fn publish(&self, id: Uuid) -> Result<MessageSender, MessageBusError> {
        self.senders
            .get(&id)
            .cloned()
            .ok_or(MessageBusError::ChannelNotFound(id))
    }

    pub fn stop(&mut self, id: Uuid) {
        self.senders.remove(&id);
        self.receivers.remove(&id);
    }

    pub fn stop_all(&mut self) {
        let ids: Vec<Uuid> = self.senders.keys().cloned().collect();
        for id in ids {
            self.stop(id);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_bus_creation() -> Result<(), MessageBusError> {
        let _message_bus = MessageBus::new();
        Ok(())
    }
    
    #[test]
    fn test_message_bus_channel_creation() -> Result<(), MessageBusError> {
        let mut message_bus = MessageBus::new();
        let _channel_id = message_bus.create_channel(0)?;
        Ok(())
    }
    
    #[test]
    fn test_message_bus_publish_subscribe() -> Result<(), MessageBusError> {
        let mut message_bus = MessageBus::new();
        let channel_id = message_bus.create_channel(0)?;
        let _publisher = message_bus.publish(channel_id)?;
        let _subscriber = message_bus.subscribe(channel_id)?;
        Ok(())
    } 

    #[test]
    #[should_panic]
    fn test_message_bus_publish_no_channel() {
        let message_bus = MessageBus::new();
        let id = Uuid::new_v4();
        let _ = message_bus.publish(id).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_message_bus_subscribe_no_channel() {
        let mut message_bus = MessageBus::new();
        let id = Uuid::new_v4();
        let _ = message_bus.subscribe(id).unwrap();
    }

    #[test]
    fn test_message_bus_stop() -> Result<(), MessageBusError> {
        let mut message_bus = MessageBus::new();
        let channel_id = message_bus.create_channel(0)?;

        assert!(message_bus.senders.get(&channel_id).is_some());
        assert!(message_bus.receivers.get(&channel_id).is_some());  

        message_bus.stop(channel_id);

        assert!(message_bus.senders.get(&channel_id).is_none());
        assert!(message_bus.receivers.get(&channel_id).is_none());  
        Ok(())
    }
}