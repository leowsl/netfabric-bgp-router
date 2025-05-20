use env_logger;
use log::info;
use netfabric_bgp::modules::network::NetworkManagerError;
use netfabric_bgp::ThreadManager;

fn test_create_and_start_network(
    thread_manager: &mut ThreadManager,
) -> Result<(), NetworkManagerError> {
    use netfabric_bgp::components::bgp::bgp_config::{ProcessConfig, SessionConfig};
    use netfabric_bgp::components::filters::{CombinedOrFilter, HostFilter};
    use netfabric_bgp::modules::live_bgp_parser::{get_parser_with_bgp_session, LiveBgpParser};
    use netfabric_bgp::modules::network::NetworkManager;
    use netfabric_bgp::modules::router::RouterOptions;
    use netfabric_bgp::utils::state_machine::StateMachine;
    use netfabric_bgp::utils::topology::NetworkTopology;

    // Network topology
    // (Live BGP Parser) ---- r1 ---- r2 --- r4
    //                        |     /         |
    //                        |   /           |
    //                        r3 ----------- r5
    let mut topology = NetworkTopology::new();
    let r1 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
    let r2 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
    let r3 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
    let r4 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
    let r5 = topology.add_router(RouterOptions::default(), ProcessConfig::default());
    topology.add_connection(
        r1,
        r2,
        (SessionConfig::default(), SessionConfig::default()),
        1000,
    );
    topology.add_connection(
        r1,
        r3,
        (SessionConfig::default(), SessionConfig::default()),
        1000,
    );
    topology.add_connection(
        r2,
        r3,
        (SessionConfig::default(), SessionConfig::default()),
        1000,
    );
    topology.add_connection(
        r2,
        r4,
        (SessionConfig::default(), SessionConfig::default()),
        1000,
    );
    topology.add_connection(
        r3,
        r5,
        (SessionConfig::default(), SessionConfig::default()),
        1000,
    );
    topology.add_connection(
        r4,
        r5,
        (SessionConfig::default(), SessionConfig::default()),
        1000,
    );

    let mut network_manager = NetworkManager::from_topology(&topology, thread_manager)?;

    // Live Bgp Parser - need to figure out how ripe processes bursts and if the buffer us sufficient
    let (parser, mut session) = get_parser_with_bgp_session(&mut network_manager)?;
    let mut second_tm = ThreadManager::new();
    let mut parser_sm = StateMachine::new(&mut second_tm, parser)?; // Temporary until we fix borrowing issues of the network manager

    // Set Filter for the parser session
    let interface = session.get_interface_mut().unwrap();
    interface.set_in_filter(CombinedOrFilter::from_vec(vec![
        HostFilter::new("rrc15.ripe.net".to_string()),
        HostFilter::new("rrc16.ripe.net".to_string()),
    ]));

    // Attach session to router 0
    let rib = network_manager.get_rib_mutex();
    let router0 = network_manager.get_router_mut(&r1).unwrap();
    let bgp_process = router0.get_bgp_processes_mut().get_mut(0).unwrap();
    bgp_process.add_session(session);
    bgp_process.set_rib(rib);

    // Start the network
    network_manager.start()?;
    parser_sm.start()?;

    std::thread::sleep(std::time::Duration::from_secs(5));

    // Stop the network - Give time for the network to stabilize
    // parser_sm.stop()?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    network_manager.stop()?;
    std::thread::sleep(std::time::Duration::from_secs(1));

    info!("Router mapping:");
    info!("Router 0: {:.4}", r1.to_string());
    info!("Router 1: {:.4}", r2.to_string());
    info!("Router 2: {:.4}", r3.to_string());
    info!("Router 3: {:.4}", r4.to_string());
    info!("Router 4: {:.4}", r5.to_string());
    info!("Router 5: {:.4}", r5.to_string());

    network_manager
        .get_rib_clone()
        .export_to_file("./data/rib.json");
    info!("{}", &network_manager.get_rib_clone());

    drop(network_manager);
    if let Some(final_state) = parser_sm.get_final_state_cloned::<LiveBgpParser>(&mut second_tm) {
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
