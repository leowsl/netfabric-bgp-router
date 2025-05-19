use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

pub trait TryLockWithTimeout<T> {
    fn try_lock_with_timeout(&self, timeout: std::time::Duration) -> Result<MutexGuard<T>, TryLockError<T>>;
}

impl<T> TryLockWithTimeout<T> for Arc<Mutex<T>> {
    fn try_lock_with_timeout(&self, timeout: std::time::Duration) -> Result<MutexGuard<T>, TryLockError<T>> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            match self.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
        Err(TryLockError::WouldBlock)
    }
} 