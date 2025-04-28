use std::thread;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::collections::HashMap;
use std::any::Any;
use std::sync::Arc;
use log::error;

pub trait Message: Any + Send + Sync + 'static {}

impl dyn Message {
    pub fn cast<T: 'static>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

pub struct MessageBus {
    senders: HashMap<u8, Arc<SyncSender<Box<dyn Message>>>>,
    receivers: HashMap<u8, Receiver<Box<dyn Message>>>
}

impl MessageBus {
    pub fn new() -> Self {
        MessageBus {
            senders: HashMap::new(),
            receivers: HashMap::new(),
        }
    }

    pub fn create_channel(&mut self, id: u8, bound: usize) -> bool {
        if self.senders.contains_key(&id) || self.receivers.contains_key(&id) {
            error!("Channel with id {} already exists", id);
            return false;
        }
        
        let (tx, rx) = sync_channel::<Box<dyn Message>>(bound);
        self.senders.insert(id, Arc::new(tx));
        self.receivers.insert(id, rx);
        return true;
    }
    
    pub fn subscribe(&mut self, id: u8) -> Option<Receiver<Box<dyn Message>>> {
        if !self.receivers.contains_key(&id) {
            error!("Couldn't find a receiver with id {}", id);
            return None;
        }
        return Some(self.receivers.remove(&id).unwrap());
    }
    
    pub fn publish(&self, id: u8) -> Option<Arc<SyncSender<Box<dyn Message>>>> {
        if !self.senders.contains_key(&id) {
            error!("Couldn't find a sender with id {}", id);
            return None;
        }
        return self.senders.get(&id).cloned();
    }

    pub fn stop(&mut self, id: u8) {
        self.senders.remove(&id);
        self.receivers.remove(&id);
    }
}

pub struct ThreadManager {
    pub thread_handles: Vec<thread::JoinHandle<()>>,
    pub message_bus: MessageBus,
}

impl ThreadManager {
    pub fn new() -> Self {
        ThreadManager {
            thread_handles: Vec::new(),
            message_bus: MessageBus::new(),
        }
    }

    pub fn start_thread<F>(&mut self, function: F) 
    where 
        F: Fn() +Send + 'static,
    {
        let handle = thread::spawn(move || function());
        self.thread_handles.push(handle);
    }

    pub fn join_all(&mut self) {
        for handle in self.thread_handles.drain(..) {
            handle.join().unwrap();
        }
    }
}