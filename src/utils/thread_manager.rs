use crate::utils::message_bus::{
    Message, MessageBus, MessageBusError, MessageReceiver, MessageSender,
};
use log::error;
use std::any::Any;
use std::collections::HashMap;
use std::sync::mpsc::SendError;
use std::sync::{Arc, Mutex};
use std::thread;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ThreadManagerError {
    #[error("Thread operation failed: {0}")]
    ThreadError(String),
    #[error("Failed to send message: {0}")]
    SendError(#[from] SendError<Box<dyn Message>>),
    #[error("Failed to lock mutex: {0}")]
    LockError(String),
    #[error("Message bus operation failed: {0}")]
    MessageBusError(String),
}

impl From<MessageBusError> for ThreadManagerError {
    fn from(value: MessageBusError) -> Self {
        Self::MessageBusError(value.to_string())
    }
}

pub type ThreadResult<T> = Result<T, ThreadManagerError>;

pub struct ThreadManager {
    thread_handles: HashMap<Uuid, thread::JoinHandle<Box<dyn Any + Send>>>,
    message_bus: Arc<Mutex<MessageBus>>,
}

impl ThreadManager {
    pub fn new() -> Self {
        ThreadManager {
            thread_handles: HashMap::new(),
            message_bus: Arc::new(Mutex::new(MessageBus::new())),
        }
    }

    pub fn start_thread<F, T>(&mut self, function: F) -> ThreadResult<Uuid>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let id = Uuid::new_v4();
        let handle = thread::spawn(move || Box::new(function()) as Box<dyn Any + Send>);
        self.thread_handles.insert(id, handle);
        Ok(id)
    }

    pub fn is_thread_running(&self, id: &Uuid) -> ThreadResult<bool> {
        match self.thread_handles.get(id) {
            Some(handle) => Ok(!handle.is_finished()),
            None => Ok(false),
        }
    }

    pub fn join_thread<T: 'static>(&mut self, id: &Uuid) -> ThreadResult<T> {
        self.thread_handles
            .remove(id)
            .ok_or(ThreadManagerError::ThreadError(format!(
                "Thread {} not found",
                id
            )))?
            .join()
            .map_err(|_| {
                ThreadManagerError::ThreadError(format!("Thread {} join operation failed", id))
            })?
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| {
                ThreadManagerError::ThreadError("Failed to downcast thread result".to_string())
            })
    }

    pub fn join_all(&mut self) -> ThreadResult<()> {
        self.thread_handles.drain().for_each(|(_, handle)| {
            handle.join().unwrap();
        });
        Ok(())
    }

    pub fn lock_message_bus(&self) -> ThreadResult<std::sync::MutexGuard<'_, MessageBus>> {
        self.message_bus.lock().map_err(|e| {
            ThreadManagerError::LockError(format!("Failed to lock message bus: {}", e))
        })
    }

    pub fn get_message_bus_channel_pair(
        &self,
        channel_capacity: usize,
    ) -> ThreadResult<(MessageSender, MessageReceiver)> {
        let (tx, rx) = if let Ok(mut message_bus) = self.lock_message_bus() {
            let channel_id = message_bus.create_channel(channel_capacity)?;
            (
                message_bus.publish(channel_id)?,
                message_bus.subscribe(channel_id)?,
            )
        } else {
            return Err(ThreadManagerError::LockError(
                "Failed to lock message bus".to_string(),
            ));
        };
        return Ok((tx, rx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_manager_creation() -> Result<(), ThreadManagerError> {
        let _thread_manager = ThreadManager::new();
        Ok(())
    }

    #[test]
    fn test_thread_manager_start_thread() -> Result<(), ThreadManagerError> {
        let mut thread_manager = ThreadManager::new();

        let x = Arc::new(Mutex::new(0));

        let x_clone = x.clone();
        let thread = thread_manager.start_thread(move || {
            *x_clone.lock().unwrap() += 1;
        })?;

        thread_manager.join_thread(&thread)?;

        println!("x: {}", *x.lock().unwrap());
        assert!(*x.lock().unwrap() == 1);
        Ok(())
    }

    #[test]
    fn test_thread_manager_start_thread_with_result() -> Result<(), ThreadManagerError> {
        let mut thread_manager = ThreadManager::new();

        let x = || -> i32 {
            let mut x = 1;
            while x < 10 {
                x += 1;
            }
            x
        };

        // With downcasting
        let thread2 = thread_manager.start_thread(x)?;
        let result2 = thread_manager.join_thread::<i32>(&thread2)?;
        assert_eq!(result2, 10);
        Ok(())
    }

    #[test]
    fn test_thread_manager_join_all() -> Result<(), ThreadManagerError> {
        let mut thread_manager = ThreadManager::new();

        let x: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
        for _ in 0..10 {
            let x_clone = x.clone();
            thread_manager.start_thread(move || {
                *x_clone.lock().unwrap() += 1;
            })?;
        }

        thread_manager.join_all()?;
        assert!(*x.lock().unwrap() == 10);
        Ok(())
    }

    #[test]
    fn test_thread_manager_lock_message_bus() -> Result<(), ThreadManagerError> {
        let thread_manager = ThreadManager::new();
        if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
            let channel = message_bus.create_channel(0).map_err(|e| {
                ThreadManagerError::LockError(format!("Failed to create channel: {}", e))
            })?;
            message_bus.stop(channel);
        }
        Ok(())
    }

    #[test]
    fn test_thread_running_status() -> Result<(), ThreadManagerError> {
        let mut thread_manager = ThreadManager::new();

        let thread_id = thread_manager.start_thread(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
        })?;

        assert!(thread_manager.is_thread_running(&thread_id)?);
        thread_manager.join_thread(&thread_id)?;
        assert!(!thread_manager.is_thread_running(&thread_id)?);

        Ok(())
    }

    #[test]
    fn test_get_message_bus_channel_pair() -> Result<(), ThreadManagerError> {
        let thread_manager = ThreadManager::new();
        let (_tx, _rx) = thread_manager.get_message_bus_channel_pair(1)?;
        Ok(())
    }
}
