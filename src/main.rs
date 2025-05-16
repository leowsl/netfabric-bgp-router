use env_logger;
use log::info;
use netfabric_bgp::modules::network::NetworkManagerError;
use netfabric_bgp::ThreadManager;

fn test_create_and_start_network(
    thread_manager: &mut ThreadManager,
) -> Result<(), NetworkManagerError> {
    use netfabric_bgp::components::advertisement::Advertisement;
    use netfabric_bgp::components::filters::{CombinedOrFilter, HostFilter};
    use netfabric_bgp::modules::live_bgp_parser::{get_parser_with_router, LiveBgpParser};
    use netfabric_bgp::modules::network::NetworkManager;
    use netfabric_bgp::utils::state_machine::StateMachine;
    use std::net::{IpAddr, Ipv4Addr};
    use netfabric_bgp::modules::router::RouterOptions;
    use uuid::Uuid;

    // ids
    let router1_id = Uuid::new_v4();
    let router2_id = Uuid::new_v4();
    let router3_id = Uuid::new_v4();

    // Live Bgp Parser - need to figure out how ripe processes bursts and if the buffer us sufficient
    let (parser, mut router0) = get_parser_with_router(thread_manager, 1000, 500)?;
    let mut parser_sm = StateMachine::new(thread_manager, parser)?;

    // Router 0 (Connected to live bgp parser)
    let router0_id = router0.id.clone();
    let host_filter: CombinedOrFilter<Advertisement> = CombinedOrFilter::from_vec(vec![
        HostFilter::new("rrc15.ripe.net".to_string()),
        HostFilter::new("rrc16.ripe.net".to_string()),
    ]);
    router0
        .get_interface_by_index_mut(0)
        .unwrap()
        .set_in_filter(host_filter);

    // Create network
    let mut network_manager = NetworkManager::new(thread_manager);
    network_manager.insert_router(router0);
    network_manager.create_router(router1_id);
    network_manager.create_router(router2_id);
    network_manager.create_router(router3_id);

    // Connections
    // 0 -> 1
    network_manager.create_router_interface_pair(
        (&router0_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 1, 1))),
        (&router1_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 1, 2))),
        250,
        250,
    )?;
    // 0 -> 2
    network_manager.create_router_interface_pair(
        (&router0_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 2, 1))),
        (&router2_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 2, 2))),
        250,
        500,
    )?;
    // 1 -> 2
    network_manager.create_router_interface_pair(
        (&router1_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 3, 1))),
        (&router2_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 3, 2))),
        250,
        500,
    )?;
    // 1 -> 3
    network_manager.create_router_interface_pair(
        (&router1_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 4, 1))),
        (&router3_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 4, 2))),
        250,
        250,
    )?;
    // 2 -> 3
    network_manager.create_router_interface_pair(
        (&router2_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 5, 1))),
        (&router3_id, &IpAddr::V4(Ipv4Addr::new(1, 0, 5, 2))),
        250,
        250,
    )?;

    // Some settings
    network_manager.get_router_mut(&router3_id).unwrap().set_options(RouterOptions {
        drop_incoming_advertisements: true,
        ..Default::default()
    });

    // Start the network
    network_manager.start()?;
    parser_sm.start()?;

    std::thread::sleep(std::time::Duration::from_secs(20));

    // Stop the network - Give time for the network to stabilize
    parser_sm.stop()?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    network_manager.stop()?;
    std::thread::sleep(std::time::Duration::from_secs(1));

    info!("Router mapping:");
    info!("Router 0: {:.4}", router0_id.to_string());
    info!("Router 1: {:.4}", router1_id.to_string());
    info!("Router 2: {:.4}", router2_id.to_string());
    info!("Router 3: {:.4}", router3_id.to_string());

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
