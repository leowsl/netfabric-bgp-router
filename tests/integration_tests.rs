use netfabric_bgp::components::advertisement::{Advertisement, AdvertisementType};
use netfabric_bgp::components::interface::Interface;
use netfabric_bgp::components::ris_live_data::{RisLiveData, RisLiveMessage};
use netfabric_bgp::modules::live_bgp_parser::get_parser_with_router;
use netfabric_bgp::modules::network::{NetworkManager, NetworkManagerError};
use netfabric_bgp::modules::router::{Router, RouterOptions};
use netfabric_bgp::utils::message_bus::Message;
use netfabric_bgp::utils::state_machine::{StateMachine, StateMachineError};
use netfabric_bgp::utils::thread_manager::ThreadManager;

use std::net::{IpAddr, Ipv4Addr};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq, Clone)]
struct TestMessage(String);
impl Message for TestMessage {}

fn init() {
    let _ = env_logger::try_init_from_env(env_logger::Env::default().filter_or("LOG_LEVEL", "info"));
}

#[test]
fn test_message_bus() -> Result<(), String> {
    let thread_manager = ThreadManager::new();
    let id = Uuid::new_v4();

    let (_tx, _rx) = if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        message_bus
            .create_channel_with_uuid(1, id)
            .map_err(|e| format!("Failed to create channel: {:?}", e))?;

        let tx = message_bus
            .publish(id)
            .map_err(|e| format!("Failed to get publisher: {:?}", e))?;
        let rx = message_bus
            .subscribe(id)
            .map_err(|e| format!("Failed to get subscriber: {:?}", e))?;
        (tx, rx)
    } else {
        return Err("Failed to lock message bus".to_string());
    };

    let (tx, rx) = thread_manager
        .get_message_bus_channel_pair(1)
        .map_err(|e| format!("Failed to get message bus channel pair: {:?}", e))?;

    let sent_msg = TestMessage("Hello, world!".to_string());
    tx.send(Box::new(sent_msg.clone()))
        .map_err(|e| format!("Failed to send message: {:?}", e))?;

    let recv_msg = rx
        .recv()
        .map_err(|e| format!("Failed to receive message: {:?}", e))?;
    let recv_test_msg = recv_msg
        .cast::<TestMessage>()
        .ok_or("Failed to cast received message")?;
    assert_eq!(recv_test_msg.0, sent_msg.0);

    if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        message_bus.stop(id);
    }

    Ok(())
}

#[test]
fn test_live_bgp_parser() -> Result<(), StateMachineError> {
    let mut thread_manager = ThreadManager::new();

    // Create parser and router
    let (parser, mut router) = get_parser_with_router(&mut thread_manager, 10)
        .map_err(|e| StateMachineError::StateMachineError(e.to_string()))?;
    router.set_options(RouterOptions {
        drop_incoming_advertisements: true,
        use_bgp_rib: false,
        ..Default::default()
    });
    let (mut parser_sm, mut router_sm) = (
        StateMachine::new(&mut thread_manager, parser)?,
        StateMachine::new(&mut thread_manager, router)?,
    );

    // Start State Machines
    router_sm.start()?;
    parser_sm.start()?;
    let parser_thread_id = parser_sm.get_runner_thread_id();
    let router_thread_id = router_sm.get_runner_thread_id();

    assert!(
        thread_manager.is_thread_running(&router_thread_id)?,
        "Router state machine should be running"
    );
    assert!(
        thread_manager.is_thread_running(&parser_thread_id)?,
        "BGP parser state machine should be running"
    );

    std::thread::sleep(std::time::Duration::from_secs(5));

    // Stop both state machines
    parser_sm.stop()?;
    router_sm.stop()?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(
        !thread_manager.is_thread_running(&router_thread_id)?,
        "Router state machine should be stopped"
    );
    assert!(
        !thread_manager.is_thread_running(&parser_thread_id)?,
        "BGP parser state machine should be stopped"
    );

    Ok(())
}

#[test]
fn test_create_and_start_network_with_live_parsing_to_rib() -> Result<(), NetworkManagerError> {
    let thread_manager = &mut ThreadManager::new();

    // Live Bgp Parser
    let (bgp_live_parser, router) = get_parser_with_router(thread_manager, 500)?;
    let mut bgp_live_sm = StateMachine::new(thread_manager, bgp_live_parser)?;

    // Create and start network
    let mut network_manager = NetworkManager::new(thread_manager);
    network_manager.insert_router(router);
    network_manager.start()?;
    bgp_live_sm.start()?;

    std::thread::sleep(std::time::Duration::from_secs(2));

    // Stop the network
    bgp_live_sm.stop()?;
    network_manager.stop()?;
    let (prefix_count, rib_entry_count) = network_manager.get_rib_clone().get_prefix_count();
    assert!(prefix_count > 0);
    assert!(rib_entry_count > 0);
    Ok(())
}
