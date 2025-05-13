use crate::components::bgp_rib::BgpRib;

pub fn bgp_rib_overview(rib: &BgpRib) -> String {
    let mut output = String::new();
    output.push_str("BGP RIB Overview:\n");
    output.push_str("┌──────────────┬──────────────┬──────────────┬──────────────┐\n");
    output.push_str("│   Router     │ IPv4 Prefixes│ IPv6 Prefixes│    Routes    │\n");
    output.push_str("├──────────────┼──────────────┼──────────────┼──────────────┤\n");

    for router in rib.get_router_ids() {
        let mut id = router.to_string();
        id.truncate(8);
        let (routes, ipv4_prefixes, ipv6_prefixes) = rib.get_routes_count_with_mask(&router);

        output.push_str(&format!(
            "│ {:<12} │ {:>12} │ {:>12} │ {:>12} │\n",
            id, ipv4_prefixes, ipv6_prefixes, routes
        ));
    }


    let (total_ipv4_prefixes, total_ipv6_prefixes) = rib.get_prefix_count();
    let total_routes = rib.get_total_route_count();

    output.push_str("├──────────────┼──────────────┼──────────────┼──────────────┤\n");
    output.push_str(&format!(
        "│ {:<12} │ {:>12} │ {:>12} │ {:>12} │\n",
        "Total", total_ipv4_prefixes, total_ipv6_prefixes, total_routes
    ));
    output.push_str("└──────────────┴──────────────┴──────────────┴──────────────┘\n");

    output
}
