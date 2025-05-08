use env_logger;
use log::info;
use netfabric_bgp::ThreadManager;

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let mut tm = ThreadManager::new();

    // Clean up
    info!("Stopping");
    std::thread::sleep(std::time::Duration::from_secs(1));

    tm.join_all().unwrap();

    if let Ok(mut message_bus) = tm.lock_message_bus() {
        message_bus.stop_all();
    } else {
        panic!("Failed to lock message bus");
    }

    drop(tm);
}
