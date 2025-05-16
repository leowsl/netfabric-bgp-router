use env_logger;
use log::info;
use netfabric_bgp::modules::network::NetworkManagerError;
use netfabric_bgp::ThreadManager;

fn test_create_and_start_network(
    thread_manager: &mut ThreadManager,
) -> Result<(), NetworkManagerError> {
    use netfabric_bgp::components::advertisement::Advertisement;
    use netfabric_bgp::components::filters::{CombinedOrFilter, HostFilter, NoFilter};
    use netfabric_bgp::modules::live_bgp_parser::{get_parser_with_router, LiveBgpParser};
    use netfabric_bgp::modules::network::NetworkManager;
    use netfabric_bgp::utils::state_machine::StateMachine;
    use uuid::Uuid;

    // ids
    let router1_id = Uuid::new_v4();
    let router2_id = Uuid::new_v4();
    let router3_id = Uuid::new_v4();

    // Live Bgp Parser
    let (parser, router0) = get_parser_with_router(thread_manager, 2000)?;
    let mut parser_sm = StateMachine::new(thread_manager, parser)?;
    let router0_id = router0.id.clone();
    let host_filter: CombinedOrFilter<Advertisement> = CombinedOrFilter::from_vec(vec![
        HostFilter::new("rrc15.ripe.net".to_string()),
        HostFilter::new("rrc16.ripe.net".to_string()),
    ]);

    // Create network
    let mut network_manager = NetworkManager::new(thread_manager);
    network_manager.insert_router(router0);
    network_manager.create_router(router1_id);
    network_manager.create_router(router2_id);
    network_manager.create_router(router3_id);

    // Connections
    // network_manager.connect_router_pair(&router0_id, &router1_id, 200, (NoFilter, NoFilter))?;
    // network_manager.connect_router_pair(&router0_id, &router2_id, 400, (NoFilter, NoFilter))?;
    // network_manager.connect_router_pair(&router1_id, &router2_id, 400, (NoFilter, NoFilter))?;
    // network_manager.connect_router_pair(&router1_id, &router3_id, 400, (NoFilter, NoFilter))?;
    // network_manager.connect_router_pair(&router2_id, &router3_id, 400, (NoFilter, NoFilter))?;

    // Start the network
    network_manager.start()?;
    parser_sm.start()?;

    std::thread::sleep(std::time::Duration::from_secs(10));

    // Stop the network - Give time for the network to stabilize
    parser_sm.stop()?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    network_manager.stop()?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    network_manager
        .get_rib_clone()
        .export_to_file("./data/rib.json");
    info!("{}", &network_manager.get_rib_clone());

    drop(network_manager);
    if let Some(final_state) = parser_sm.get_final_state_cloned::<LiveBgpParser>(thread_manager) {
        info!("{}", final_state.get_statistics());
    }

    Ok(())
}

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let mut tm = ThreadManager::new();

    let _ = test_create_and_start_network(&mut tm);

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
