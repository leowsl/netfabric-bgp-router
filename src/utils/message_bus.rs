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
