use netfabric_bgp::utils::thread_manager::ThreadManager;
use netfabric_bgp::utils::message_bus::{Message, MessageSender, MessageReceiver};
use netfabric_bgp::utils::state_machine::{StateMachine,StateMachineError};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq, Clone)]
struct TestMessage(String);
impl Message for TestMessage {}

#[test]
fn test_message_bus() -> Result<(), String> {
    let thread_manager = ThreadManager::new();
    let id = Uuid::new_v4();
    
    let (tx, rx) = if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        message_bus.create_channel_with_uuid(1, id)
            .map_err(|e| format!("Failed to create channel: {:?}", e))?;
            
        let tx = message_bus.publish(id).map_err(|e| format!("Failed to get publisher: {:?}", e))?;
        let rx = message_bus.subscribe(id).map_err(|e| format!("Failed to get subscriber: {:?}", e))?;
        (tx, rx)
    } else {
        return Err("Failed to lock message bus".to_string());
    };

    let sent_msg = TestMessage("Hello, world!".to_string());
    tx.send(Box::new(sent_msg.clone())).map_err(|e| format!("Failed to send message: {:?}", e))?;
    
    let recv_msg = rx.recv().map_err(|e| format!("Failed to receive message: {:?}", e))?;
    let recv_test_msg = recv_msg.cast::<TestMessage>().ok_or("Failed to cast received message")?;
    assert_eq!(recv_test_msg.0, sent_msg.0);
    
    if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        message_bus.stop(id);
    }
    
    Ok(())
}

#[test]
fn test_live_bgp_parser() -> Result<(), StateMachineError> {
    use netfabric_bgp::components::live_bgp_parser::LiveBgpParser;
    use netfabric_bgp::utils::thread_manager::ThreadManagerError;
    use netfabric_bgp::components::router::Router;

    let mut thread_manager = ThreadManager::new();

    // Create channel for BGP messages
    let (tx, rx) = if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        let channel_id = message_bus.create_channel(10000).unwrap();
        (
            message_bus.publish(channel_id)?,
            message_bus.subscribe(channel_id)?,
        )
    } else {
        return Err(StateMachineError::ThreadManagerError(ThreadManagerError::LockError("Failed to lock message bus.".to_string())));
    };

    // Create and start router
    let mut router = Router::new(Uuid::new_v4());
    router.add_receiver(rx);
    let mut router_state_machine = StateMachine::new(&mut thread_manager, router)?;
    let router_thread_id = router_state_machine.get_runner_thread_id();
    router_state_machine.start()?;

    // Create and start BGP parser
    let live_bgp_parser = LiveBgpParser::new(tx);
    let mut parser_state_machine = StateMachine::new(&mut thread_manager, live_bgp_parser)?;
    let parser_thread_id = parser_state_machine.get_runner_thread_id();
    parser_state_machine.start()?;

    assert!(thread_manager.is_thread_running(router_thread_id)?, "Router state machine should be running");
    assert!(thread_manager.is_thread_running(parser_thread_id)?, "BGP parser state machine should be running");

    std::thread::sleep(std::time::Duration::from_secs(5));

    // Stop both state machines
    parser_state_machine.stop()?;
    router_state_machine.stop()?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(!thread_manager.is_thread_running(router_thread_id)?, "Router state machine should be stopped");
    assert!(!thread_manager.is_thread_running(parser_thread_id)?, "BGP parser state machine should be stopped");
    
    Ok(())
}

#[test]
fn test_router() -> Result<(), StateMachineError> {
    use netfabric_bgp::components::live_bgp_parser::RisLiveData;
    use netfabric_bgp::components::live_bgp_parser::RisLiveMessage;
    use netfabric_bgp::components::router::Router;
    
    let mut thread_manager = ThreadManager::new();

    let mut router = Router::new(Uuid::new_v4());
    let (tx, rx) = if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        let channel_id = message_bus.create_channel(1000).unwrap();
        (
            message_bus.publish(channel_id).unwrap(),
            message_bus.subscribe(channel_id).unwrap(),
        )
    } else {
        panic!("Failed to lock message bus");
    };
    router.add_receiver(rx);

    let mut state_machine = StateMachine::new(&mut thread_manager, router)?;
    let thread_id = state_machine.get_runner_thread_id();
    state_machine.start()?;

    assert!(thread_manager.is_thread_running(thread_id)?, "Router state machine should be running");

    let test_message = RisLiveMessage {
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
    };

    tx.send(Box::new(test_message))
        .map_err(|e| StateMachineError::StateMachineError(e.to_string()))?;

    std::thread::sleep(std::time::Duration::from_secs(1));

    state_machine.stop()?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(!thread_manager.is_thread_running(thread_id)?, "Router state machine should be stopped");
    
    Ok(())
}

#[test]
fn test_create_and_start_network_with_live_parsing_to_rib() -> Result<(), StateMachineError> {
    use netfabric_bgp::components::network::NetworkManager;
    use netfabric_bgp::components::live_bgp_parser::LiveBgpParser;
    use netfabric_bgp::components::router::Router;
    use netfabric_bgp::utils::state_machine::StateMachine;
    use uuid::Uuid;

    let thread_manager = &mut ThreadManager::new();

    // Channel for the bgp parser    
    let (bgp_parser_tx, bgp_parser_rx) = if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        let channel_id = message_bus.create_channel(10000).unwrap();
        (
            message_bus.publish(channel_id).unwrap(),
            message_bus.subscribe(channel_id).unwrap(),
        )
    } else {
        panic!("Failed to lock message bus");
    };
    
    // Bgp parser state machine
    let bgp_parser = LiveBgpParser::new(bgp_parser_tx);
    let mut bgp_parser_state_machine = StateMachine::new(thread_manager, bgp_parser)?;    
    bgp_parser_state_machine.start()?;
    
    // Router state machine
    let mut router = Router::new(Uuid::new_v4());
    router.add_receiver(bgp_parser_rx);

    // Create and start network
    let mut network_manager = NetworkManager::new(thread_manager);
    network_manager.insert_router(router);
    network_manager.start()?;

    std::thread::sleep(std::time::Duration::from_secs(1));

    // Stop the network
    bgp_parser_state_machine.stop()?;
    network_manager.stop()?;
    let (prefix_count, rib_entry_count) = network_manager.get_rib().get_prefix_count();
    assert!(prefix_count > 0);
    assert!(rib_entry_count > 0);
    Ok(())
}