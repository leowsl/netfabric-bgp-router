mod components;
mod utils;

use components::{live_bgp_parser, router::Router};
use env_logger;
use utils::thread_manager::ThreadManager;

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut tm: ThreadManager = ThreadManager::new();

    // Create a channel for the live BGP parser
    if let Some(id) = tm.message_bus.create_channel(5) {
        let tx = tm.message_bus.publish(id).unwrap();
        tm.start_thread(move || live_bgp_parser::main(tx));

        let rx = tm.message_bus.subscribe(id).unwrap();
        tm.start_thread(move || Router::new(0, rx).start());
    }

    // Set 1 sec timeout
    std::thread::sleep(std::time::Duration::from_secs(1));

    //TODO: Actually stop the threads

    tm.message_bus.stop_all();
    tm.join_all();
    return;
}
