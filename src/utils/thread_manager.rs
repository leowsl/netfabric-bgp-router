use std::thread;
use std::sync::mpsc::Receiver;

pub struct ThreadManager {
    thread_handles: Vec<thread::JoinHandle<()>>,
}

impl ThreadManager {
    pub fn new() -> Self {
        ThreadManager {
            thread_handles: Vec::new()
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