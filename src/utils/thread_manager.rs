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
        let handle = thread::spawn(move || function());
        self.thread_handles
            .lock()
            .map_err(|e| {
                ThreadManagerError::LockError(format!("Failed to lock thread handles: {}", e))
            })?
            .insert(id, handle);
        return Ok(id);
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
