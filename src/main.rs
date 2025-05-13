use env_logger;
use log::info;
use netfabric_bgp::modules::network::NetworkManagerError;
use netfabric_bgp::ThreadManager;

fn test_create_and_start_network(
    thread_manager: &mut ThreadManager,
) -> Result<(), NetworkManagerError> {
    use netfabric_bgp::components::filters::FilterHost;
    use netfabric_bgp::modules::live_bgp_parser::create_parser_router_pair;
    use netfabric_bgp::modules::network::NetworkManager;
    use netfabric_bgp::modules::router::RouterOptions;
    use netfabric_bgp::utils::state_machine::StateMachine;
    use uuid::Uuid;

    // ids
    let router1_id = Uuid::new_v4();
    let router2_id = Uuid::new_v4();
    let router3_id = Uuid::new_v4();

    // Live Bgp Parser
    let host_filter = Box::new(FilterHost::new("rrc15.ripe.net".to_string()));
    let (bgp_live_parser, mut bgp_live_parser_router) =
        create_parser_router_pair(thread_manager, 500, vec![host_filter])?;
    let mut bgp_live_sm = StateMachine::new(thread_manager, bgp_live_parser)?;
    let router0_id = bgp_live_parser_router.id.clone();
    bgp_live_parser_router.set_options(RouterOptions {
        use_bgp_rib: false,
        ..Default::default()
    });

    // Create and start network
    let mut network_manager = NetworkManager::new(thread_manager);
    network_manager.insert_router(bgp_live_parser_router);
    network_manager.create_router(router1_id);
    network_manager.create_router(router2_id);
    network_manager.create_router(router3_id);

    // Connections
    network_manager.connect_router_pair(&router0_id, &router1_id, false, 500)?;
    network_manager.connect_router_pair(&router0_id, &router2_id, false, 500)?;
    network_manager.connect_router_pair(&router1_id, &router2_id, false, 500)?;
    network_manager.connect_router_pair(&router1_id, &router3_id, false, 500)?;
    network_manager.connect_router_pair(&router2_id, &router3_id, false, 500)?;

    network_manager.start()?;
    bgp_live_sm.start()?;

    std::thread::sleep(std::time::Duration::from_secs(60));

    // Stop the network
    bgp_live_sm.stop()?;
    network_manager.stop()?;
    network_manager.get_rib().export_to_file("./data/rib.json");
    info!(
        "Final Rib Size: {:?}",
        network_manager.get_rib().get_prefix_count()
    );
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
