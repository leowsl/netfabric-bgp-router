use crate::utils::message_bus::{Message, MessageBus};
use log::error;
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
}

pub type ThreadResult<T> = Result<T, ThreadManagerError>;

pub struct ThreadManager {
    thread_handles: Arc<Mutex<HashMap<Uuid, thread::JoinHandle<()>>>>,
    message_bus: Arc<Mutex<MessageBus>>,
}

impl ThreadManager {
    pub fn new() -> Self {
        ThreadManager {
            thread_handles: Arc::new(Mutex::new(HashMap::new())),
            message_bus: Arc::new(Mutex::new(MessageBus::new())),
        }
    }

    pub fn start_thread<F>(&mut self, function: F) -> ThreadResult<Uuid>
    where
        F: FnOnce() + Send + 'static,
    {
        let id = Uuid::new_v4();
        let handle = thread::spawn(function);
        self.thread_handles
            .lock()
            .map_err(|e| {
                ThreadManagerError::LockError(format!("Failed to lock thread handles: {}", e))
            })?
            .insert(id, handle);
        Ok(id)
    }

    pub fn is_thread_running(&self, id: Uuid) -> ThreadResult<bool> {
        let handles = self.thread_handles
            .lock()
            .map_err(|e| {
                ThreadManagerError::LockError(format!("Failed to lock thread handles: {}", e))
            })?;
        
        if let Some(handle) = handles.get(&id) {
            Ok(!handle.is_finished())
        } else {
            Ok(false)
        }
    }

    pub fn join_thread(&mut self, id: Uuid) -> ThreadResult<()> {
        self.thread_handles
            .lock()
            .map_err(|e| {
                ThreadManagerError::LockError(format!("Failed to lock thread handles: {}", e))
            })?
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
        let uuids: Vec<Uuid> = self
            .thread_handles
            .lock()
            .map_err(|e| {
                ThreadManagerError::LockError(format!("Failed to lock thread handles: {}", e))
            })?
            .keys()
            .cloned()
            .collect();
        for uuid in uuids {
            self.join_thread(uuid)?;
        }
        Ok(())
    }

    pub fn lock_message_bus(&self) -> ThreadResult<std::sync::MutexGuard<'_, MessageBus>> {
        self.message_bus.lock().map_err(|e| {
            ThreadManagerError::LockError(format!("Failed to lock message bus: {}", e))
        })
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

        thread_manager.join_thread(thread)?;

        println!("x: {}", *x.lock().unwrap());
        assert!(*x.lock().unwrap() == 1);
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

        assert!(thread_manager.is_thread_running(thread_id)?);
        thread_manager.join_thread(thread_id)?;
        assert!(!thread_manager.is_thread_running(thread_id)?);

        Ok(())
    }
}
