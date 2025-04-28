use std::thread;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::collections::HashMap;
use std::any::Any;

pub trait Message: Any + Send + Sync + 'static {}

impl dyn Message {
    pub fn cast<T: 'static>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

pub struct MessageBus {
    channels: HashMap<u8, Sender<Box<dyn Message>>>
}

impl MessageBus {
    pub fn new() -> Self {
        MessageBus {
            channels: HashMap::new(),
        }
    }
    
    pub fn subscribe(&mut self, id: u8) -> Receiver<Box<dyn Message>> {
        let (tx, rx) = channel::<Box<dyn Message>>();
        self.channels.insert(id, tx);
        return rx;
    }
    
    pub fn publish(&self, id: u8) -> Sender<Box<dyn Message>> {
        return self.channels.get(&id).unwrap().clone();
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

    pub fn start_consumer<T, F>(&mut self, function: F, reveiver: Receiver<T>) 
    where 
        T: Send + 'static,
        F: Fn(Receiver<T>) + Send + 'static,
    {
        let handle = thread::spawn(move || {
            function(reveiver);
        });
        self.thread_handles.push(handle);
    }

    pub fn join_all(&mut self) {
        for handle in self.thread_handles.drain(..) {
            handle.join().unwrap();
        }
    }
}