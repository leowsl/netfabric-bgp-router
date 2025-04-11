mod utils;
mod components;

use utils::thread_manager::ThreadManager;
use std::thread;
use std::time::Duration;
use components::router;
use std::sync::mpsc::channel;

use components::live_bgp_parser;

#[tokio::main]
async fn main() {
    println!("Calling stream handler");
    live_bgp_parser::start_stream().await;
}

// fn main() {
//     let mut tm: ThreadManager = ThreadManager::new();
//     let (tx, rx) = channel::<i32>();
//     tm.start_consumer(router::start, rx);
    
//     thread::sleep(Duration::from_secs(1));
//     tx.send(1).unwrap();
//     thread::sleep(Duration::from_secs(1));
//     tx.send(2).unwrap();
//     thread::sleep(Duration::from_secs(1));
//     tx.send(3).unwrap();

//     drop(tx);

//     tm.join_all();
// }
