use log::error;
use std::any::Any;
use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, Receiver, SendError, SyncSender};
use std::sync::Arc;
use std::thread;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ThreadManagerError {
    #[error("Thread operation failed: {0}")]
    ThreadError(String),
    #[error("Channel with id {0} already exists")]
    ChannelExists(Uuid),
    #[error("Channel with id {0} not found")]
    ChannelNotFound(Uuid),
    #[error("Failed to send message: {0}")]
    SendError(#[from] SendError<Box<dyn Message>>),
}

pub type ThreadResult<T> = Result<T, ThreadManagerError>;

pub type MessageSender = Arc<SyncSender<Box<dyn Message>>>;
pub type MessageReceiver = Receiver<Box<dyn Message>>;

pub trait Message: Any + Send + Sync + 'static {}
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

    pub fn create_channel(&mut self, bound: usize) -> ThreadResult<Uuid> {
        let uuid = Uuid::new_v4();
        self.create_channel_with_uuid(bound, uuid)?;
        Ok(uuid)
    }

    pub fn create_channel_with_uuid(&mut self, bound: usize, uuid: Uuid) -> ThreadResult<()> {
        if self.senders.contains_key(&uuid) || self.receivers.contains_key(&uuid) {
            return Err(ThreadManagerError::ChannelExists(uuid));
        }
        let (tx, rx) = sync_channel::<Box<dyn Message>>(bound);
        self.senders.insert(uuid, Arc::new(tx));
        self.receivers.insert(uuid, rx);
        Ok(())
    }

    pub fn subscribe(&mut self, id: Uuid) -> ThreadResult<MessageReceiver> {
        self.receivers
            .remove(&id)
            .ok_or(ThreadManagerError::ChannelNotFound(id))
    }

    pub fn publish(&self, id: Uuid) -> ThreadResult<MessageSender> {
        self.senders
            .get(&id)
            .cloned()
            .ok_or(ThreadManagerError::ChannelNotFound(id))
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

pub struct ThreadManager {
    pub thread_handles: HashMap<Uuid, thread::JoinHandle<()>>,
    pub message_bus: MessageBus,
}

impl ThreadManager {
    pub fn new() -> Self {
        ThreadManager {
            thread_handles: HashMap::new(),
            message_bus: MessageBus::new(),
        }
    }

    pub fn start_thread<F>(&mut self, function: F) -> ThreadResult<Uuid>
    where
        F: FnOnce() + Send + 'static,
    {
        let id = Uuid::new_v4();
        let handle = thread::spawn(move || function());
        self.thread_handles.insert(id, handle);
        return Ok(id);
    }

    pub fn join_thread(&mut self, id: Uuid) -> ThreadResult<()> {
        self.thread_handles
            .remove(&id)
            .ok_or(ThreadManagerError::ThreadError(format!(
                "Thread {} not found",
                id
            )))?
            .join()
            .map_err(|_| {
                ThreadManagerError::ThreadError(format!("Thread {} join operation failed", id))
            })
    }

    pub fn join_all(&mut self) -> ThreadResult<()> {
        let uuids: Vec<Uuid> = self.thread_handles.keys().cloned().collect();
        for uuid in uuids {
            self.join_thread(uuid)?;
        }
        Ok(())
    }
}
