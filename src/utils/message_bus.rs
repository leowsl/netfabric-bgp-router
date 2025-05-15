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
    #[error("Receive error: {0}")]
    RecvError(#[from] std::sync::mpsc::TryRecvError),
}

pub trait Message: Any + Send + Sync + 'static {}
pub type MessageSender = Arc<SyncSender<Box<dyn Message>>>;
pub type MessageReceiver = Receiver<Box<dyn Message>>;

/// Trait to extend MessageReceiver with retry functionality
pub trait MessageReceiverExt {
    fn receive_with_retry(&self, max_retries: u32, retry_delay_ms: u64) -> Result<Box<dyn Message>, std::sync::mpsc::TryRecvError>;
}

impl MessageReceiverExt for MessageReceiver {
    fn receive_with_retry(&self, max_retries: u32, retry_delay_ms: u64) -> Result<Box<dyn Message>, std::sync::mpsc::TryRecvError> {
        let mut retries = 0;
        loop {
            match self.try_recv() {
                Ok(msg) => return Ok(msg),
                Err(e) => {
                    if retries >= max_retries { return Err(e); }
                    std::thread::sleep(std::time::Duration::from_millis(retry_delay_ms));
                    retries += 1;
                }
            }
        }
    }
}

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

    pub fn channel_exists(&self, id: Uuid) -> bool {
        self.senders.contains_key(&id)
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
    use std::thread;
    use std::time::Duration;

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
    fn test_message_bus_channel_exists() -> Result<(), MessageBusError> {
        let mut message_bus = MessageBus::new();
        let channel_id = message_bus.create_channel(0)?;
        assert!(message_bus.channel_exists(channel_id));
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

    #[test]
    fn test_receive_with_retry() -> Result<(), MessageBusError> {
        #[derive(Debug)]
        struct TestMessage(String);
        impl Message for TestMessage {}
        
        let mut message_bus = MessageBus::new();
        let channel_id = message_bus.create_channel(1)?;
        let publisher = message_bus.publish(channel_id)?;
        let subscriber = message_bus.subscribe(channel_id)?;
        
        // Send messages after a short delay
        let publisher_clone = publisher.clone();
        thread::spawn(move || {
            for _ in 0..10 {
                thread::sleep(Duration::from_millis(50));
                publisher_clone.send(Box::new(TestMessage("hello".to_string()))).unwrap();
            }
        });
        
        // Receive with retry
        for _ in 0..10 {
            let msg = subscriber.receive_with_retry(3, 20)?;
            let test_msg = msg.cast::<TestMessage>().expect("Failed to cast message");
            assert_eq!(test_msg.0, "hello");
        }
        
        Ok(())
    }
}