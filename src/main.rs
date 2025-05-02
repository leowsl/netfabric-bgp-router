use netfabric_bgp::{ThreadManager, StateMachine, Router, live_bgp_parser};
use env_logger;
use log::info;
use uuid;

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut tm = ThreadManager::new();

    // Testing the state machine
    let mut state_machine = StateMachine::new(&mut tm).unwrap();
    let runner_thread_id = state_machine.get_runner_thread_id();
    info!("Thread running is {}", tm.is_thread_running(runner_thread_id).unwrap());
    std::thread::sleep(std::time::Duration::from_millis(10));
    let _ = state_machine.start();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let _ = state_machine.pause();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let _ = state_machine.resume();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let _ = state_machine.stop();
    info!("Thread running is {}", tm.is_thread_running(runner_thread_id).unwrap());
    tm.join_thread(runner_thread_id).unwrap();
    info!("Thread running is {}", tm.is_thread_running(runner_thread_id).unwrap());

    // Testing the message bus
    // let (tx, rx) = if let Ok(mut message_bus) = tm.lock_message_bus() {
    //     let channel_id = message_bus.create_channel(0).unwrap();
    //     (
    //         message_bus.publish(channel_id).unwrap(),
    //         message_bus.subscribe(channel_id).unwrap(),
    //     )
    // } else {
    //     panic!("Failed to lock message bus");
    // };

    // let _ = tm.start_thread(move || live_bgp_parser::main(tx));
    // let _ = tm.start_thread(move || Router::new(uuid::Uuid::new_v4(), rx).start());

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
