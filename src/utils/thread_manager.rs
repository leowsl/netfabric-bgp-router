use std::thread;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::collections::HashMap;
use std::any::Any;
use std::sync::Arc;
use log::error;
use uuid::Uuid;

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
    receivers: HashMap<Uuid, MessageReceiver>
}

impl MessageBus {
    pub fn new() -> Self {
        MessageBus {
            senders: HashMap::new(),
            receivers: HashMap::new(),
        }
    }

    pub fn create_channel(&mut self, bound: usize) -> Option<Uuid> {
        let uuid = Uuid::new_v4();
        if self.create_channel_with_uuid(bound, uuid) {
            return Some(uuid);
        }
        return None;
    }

    pub fn create_channel_with_uuid(&mut self, bound: usize, uuid: Uuid) -> bool {
        if self.senders.contains_key(&uuid) || self.receivers.contains_key(&uuid) {
            error!("Channel with id {} already exists", uuid);
            return false;
        }
        let (tx, rx) = sync_channel::<Box<dyn Message>>(bound);
        self.senders.insert(uuid, Arc::new(tx));
        self.receivers.insert(uuid, rx);
        return true;
    }
    
    pub fn subscribe(&mut self, id: Uuid) -> Option<MessageReceiver> {
        if !self.receivers.contains_key(&id) {
            error!("Couldn't find a receiver with id {}", id);
            return None;
        }
        return Some(self.receivers.remove(&id).unwrap());
    }
    
    pub fn publish(&self, id: Uuid) -> Option<MessageSender> {
        if !self.senders.contains_key(&id) {
            error!("Couldn't find a sender with id {}", id);
            return None;
        }
        return self.senders.get(&id).cloned();
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

    pub fn start_thread<F>(&mut self, function: F) -> Uuid
    where 
        F: FnOnce() + Send + 'static,
    {
        let id = Uuid::new_v4();
        let handle = thread::spawn(move || function());
        self.thread_handles.insert(id, handle);
        return id;
    }

    pub fn join_thread(&mut self, id: Uuid) {
        if let Some(handle) = self.thread_handles.remove(&id) {
            handle.join().unwrap();
        }
    }

    pub fn join_all(&mut self) {
        let uuids: Vec<Uuid> = self.thread_handles.keys().cloned().collect();
        for uuid in uuids {
            self.join_thread(uuid);
        }
    }
}