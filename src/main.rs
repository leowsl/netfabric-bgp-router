mod utils;
mod components;

use components::{live_bgp_parser, router};
use std::{sync::mpsc::channel, thread};
use env_logger;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(
        env_logger::Env::default().default_filter_or("info")
    );

    let (tx, rx) = channel();
    let router_handle = thread::spawn(|| router::start(0, rx));

    if let Err(e) = live_bgp_parser::start_stream(tx).await {
        eprintln!("BGP stream error: {}", e);
    }

    router_handle.join().unwrap();
}
