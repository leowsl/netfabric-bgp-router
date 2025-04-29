mod components;
mod utils;

use components::{live_bgp_parser, router::Router};
use env_logger;
use utils::state_machine::StateMachine;
use utils::thread_manager::ThreadManager;

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut tm = ThreadManager::new();

    // Testing the state machine
    if let Ok(mut state_machine) = StateMachine::new(&mut tm) {
        state_machine.start(&mut tm);
        std::thread::sleep(std::time::Duration::from_secs(1));
        state_machine.stop();
        tm.join_all().unwrap();
    }

    // Testing the message bus
    let (tx, rx) = if let Ok(mut message_bus) = tm.lock_message_bus() {
        let channel_id = message_bus.create_channel(0).unwrap();
        (
            message_bus.publish(channel_id).unwrap(),
            message_bus.subscribe(channel_id).unwrap(),
        )
    } else {
        panic!("Failed to lock message bus");
    };

    let _ = tm.start_thread(move || live_bgp_parser::main(tx));
    let _ = tm.start_thread(move || Router::new(uuid::Uuid::new_v4(), rx).start());

    // Set 1 sec timeout
    std::thread::sleep(std::time::Duration::from_secs(1));

    tm.join_all().unwrap();

    if let Ok(mut message_bus) = tm.lock_message_bus() {
        message_bus.stop_all();
    } else {
        panic!("Failed to lock message bus");
    }

    drop(tm);
}
