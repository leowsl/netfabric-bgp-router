use env_logger;
use log::info;
use netfabric_bgp::utils::state_machine::StateMachineError;
use netfabric_bgp::{Router, StateMachine, ThreadManager};
use uuid::Uuid;

fn test_router(thread_manager: &mut ThreadManager) -> Result<(), StateMachineError> {
    use netfabric_bgp::components::live_bgp_parser::RisLiveData;
    use netfabric_bgp::components::live_bgp_parser::RisLiveMessage;

    let mut router = Router::new(Uuid::new_v4());
    let (tx, rx) = if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        let channel_id = message_bus.create_channel(0).unwrap();
        (
            message_bus.publish(channel_id).unwrap(),
            message_bus.subscribe(channel_id).unwrap(),
        )
    } else {
        panic!("Failed to lock message bus");
    };
    router.add_receiver(rx);

    let mut state_machine = StateMachine::new(thread_manager, router)?;
    state_machine.start()?;

    tx.send(Box::new(RisLiveMessage {
        msg_type: "RisLiveMessage".to_string(),
        data: RisLiveData {
            timestamp: 0.0,
            peer: "192.168.1.1".to_string(),
            peer_asn: "1".to_string(),
            id: "test".to_string(),
            host: "test".to_string(),
            msg_type: "RisLiveMessage".to_string(),
            path: None,
            community: None,
            origin: None,
            announcements: None,
            raw: None,
            withdrawals: None,
        },
    }))
    .map_err(|e| StateMachineError::StateMachineError(e.to_string()))?;

    std::thread::sleep(std::time::Duration::from_secs(1));
    state_machine.stop()?;
    Ok(())
}

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut tm = ThreadManager::new();

    test_router(&mut tm).unwrap();

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
