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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    
    let (tx, rx) = channel::<live_bgp_parser::RisLiveMessage>();
    
    // Start the router in a separate thread
    let router_handle = thread::spawn(move || {
        router::start(0, rx);
    });

    // Start the BGP stream
    println!("Starting BGP stream handler");
    if let Err(e) = live_bgp_parser::start_stream(tx).await {
        eprintln!("Error in BGP stream: {}", e);
    }

    // Wait for the router to finish (it won't unless there's an error)
    router_handle.join().unwrap();
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
