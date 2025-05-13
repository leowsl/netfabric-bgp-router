use netfabric_bgp::components::advertisement::AdvertisementType;
use netfabric_bgp::modules::network::NetworkManagerError;
use netfabric_bgp::utils::message_bus::Message;
use netfabric_bgp::utils::state_machine::{StateMachine, StateMachineError};
use netfabric_bgp::utils::thread_manager::ThreadManager;
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq, Clone)]
struct TestMessage(String);
impl Message for TestMessage {}

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
    use netfabric_bgp::modules::live_bgp_parser::create_parser_router_pair;
    use netfabric_bgp::utils::state_machine::StateMachine;

    let mut thread_manager = ThreadManager::new();

    // Create parser and router
    let (parser, router) = create_parser_router_pair(&mut thread_manager, 10, vec![])?;
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
fn test_router() -> Result<(), StateMachineError> {
    use netfabric_bgp::components::ris_live_data::{RisLiveData, RisLiveMessage};
    use netfabric_bgp::modules::router::{Router, RouterChannel, RouterConnection};

    let mut thread_manager = ThreadManager::new();

    let mut router = Router::new(Uuid::new_v4());
    let (tx, rx) = thread_manager.get_message_bus_channel_pair(1000)?;
    let router_connection = RouterConnection {
        channel: RouterChannel::Inbound(rx),
        filters: Vec::new(),
    };
    router.add_connection(router_connection);

    let mut state_machine = StateMachine::new(&mut thread_manager, router)?;
    let thread_id = state_machine.get_runner_thread_id();
    state_machine.start()?;

    assert!(
        thread_manager.is_thread_running(&thread_id)?,
        "Router state machine should be running"
    );

    let test_message = RisLiveMessage {
        msg_type: "RisLiveMessage".to_string(),
        data: RisLiveData {
            timestamp: 0.0,
            peer: "192.168.1.1".to_string(),
            peer_asn: "1".to_string(),
            id: "test".to_string(),
            host: "test".to_string(),
            msg_type: AdvertisementType::Update,
            path: None,
            community: None,
            origin: None,
            announcements: None,
            raw: None,
            withdrawals: None,
            aggregator: None,
            asn: None,
            capabilities: None,
            med: None,
            direction: None,
            version: None,
            hold_time: None,
            router_id: None,
            notification: None,
            state: None,
        },
    };

    tx.send(Box::new(test_message))
        .map_err(|e| StateMachineError::StateMachineError(e.to_string()))?;

    std::thread::sleep(std::time::Duration::from_secs(1));

    state_machine.stop()?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(
        !thread_manager.is_thread_running(&thread_id)?,
        "Router state machine should be stopped"
    );

    Ok(())
}

#[test]
fn test_create_and_start_network_with_live_parsing_to_rib() -> Result<(), NetworkManagerError> {
    use netfabric_bgp::modules::live_bgp_parser::create_parser_router_pair;
    use netfabric_bgp::modules::network::NetworkManager;
    use netfabric_bgp::utils::state_machine::StateMachine;

    let thread_manager = &mut ThreadManager::new();

    // Live Bgp Parser
    let (bgp_live_parser, bgp_live_parser_router) =
        create_parser_router_pair(thread_manager, 500, vec![])?;
    let mut bgp_live_sm = StateMachine::new(thread_manager, bgp_live_parser)?;

    // Create and start network
    let mut network_manager = NetworkManager::new(thread_manager);
    network_manager.insert_router(bgp_live_parser_router);
    network_manager.start()?;
    bgp_live_sm.start()?;

    std::thread::sleep(std::time::Duration::from_secs(2));

    // Stop the network
    bgp_live_sm.stop()?;
    network_manager.stop()?;
    let (prefix_count, rib_entry_count) = network_manager.get_rib().get_prefix_count();
    assert!(prefix_count > 0);
    assert!(rib_entry_count > 0);
    Ok(())
}
