use crate::components::advertisement::Advertisement;
use crate::components::bgp::{bgp_bestroute::BestRoute, bgp_rib_entry::BgpRibEntry};
use crate::components::route::Route;
use crate::utils::mutex_utils::TryLockWithTimeout;
use crate::utils::router_mask::RouterMaskMap;
use ip_network::IpNetwork;
use ip_network_table::IpNetworkTable;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct BgpRibInterface {
    router_id: Uuid,
    rib: Arc<Mutex<BgpRib>>,
}

impl BgpRibInterface {
    pub fn new(rib: Arc<Mutex<BgpRib>>, id: Uuid) -> Self {
        rib.lock().unwrap().register_router(&id);
        Self { router_id: id, rib }
    }

    pub fn update(&mut self, advertisements: Vec<Advertisement>) {
        for advertisement in advertisements {
            let mut bgp_rib_lock = self
                .rib
                .try_lock_with_timeout(std::time::Duration::from_millis(100))
                .unwrap();

            let announcements = advertisement.get_announcements();
            let withdrawals = advertisement.get_withdrawals();

            let bestroute_announcements: Vec<BestRoute> = announcements
                .iter()
                .filter_map(|route| bgp_rib_lock.insert_route(route, &self.router_id))
                .collect();

            let bestroute_withdrawals: Vec<BestRoute> = withdrawals
                .iter()
                .filter_map(|route| bgp_rib_lock.remove_route(route, &self.router_id))
                .collect();

            drop(bgp_rib_lock);
            drop(bestroute_announcements);
            drop(bestroute_withdrawals);
        }
    }
}

/*
TODO

So the current implementation with the router map is not optimal.
When we have a lot of routes, that are very similar, but only differ in small details,
we will have a lot of entries in the router map.

Idea: In the rib entry we have fixed attributes for a route, like peer, as_path, etc.
For fields that are variable like local pref, we instead use a sized vector that represents a one to one correspondence to the router mask.
We can resize this vector, depending on the mask. To avoid unnecessary memory allocations, we can try to approximate the shared value size of a router mask.

Another optimization is to use one bgp rib for eBGP and one for each iBGP session. This depends a bit on the network topology.
We should try to provide a flexible structure to ensure we can adjust to the needs of different network topologies.

Another optimization is to sort the entries for each prefix to accelerate bestroute lookups.
*/

pub struct BgpRib {
    router_mask_map: RouterMaskMap,
    treebitmap: IpNetworkTable<BgpRibEntry>,
    // router_mapping: HashMap<RouterMask, String>,
    // best_routes: IpNetworkTable<BgpRibTreebitmapEntry>,
}

impl BgpRib {
    pub fn new() -> Self {
        Self {
            router_mask_map: RouterMaskMap::new(),
            treebitmap: IpNetworkTable::new(),
            // router_mapping: HashMap::new(),
            // best_routes: HashMap::new(),
        }
    }

    pub fn get_prefix_count(&self) -> (usize, usize) {
        self.treebitmap.len()
    }

    pub fn get_router_ids(&self) -> Vec<Uuid> {
        self.router_mask_map.get_all_ids()
    }

    pub fn iter(&self) -> impl Iterator<Item = (IpNetwork, &BgpRibEntry)> {
        self.treebitmap.iter()
    }

    pub fn get_total_route_count(&self) -> u64 {
        self.treebitmap
            .iter()
            .map(|(_, entry)| entry.len() as u64)
            .sum()
    }

    pub fn get_routes_count_with_mask(&self, router_mask: &Uuid) -> (u64, u64, u64) {
        let mut route_counter: u64 = 0;
        let mut ipv4_counter: u64 = 0;
        let mut ipv6_counter: u64 = 0;
        let router_mask = self.router_mask_map.try_get(router_mask);
        self.treebitmap.iter().for_each(|(prefix, entry)| {
            let entry_len = entry.get_with_mask(router_mask).len() as u64;
            route_counter += entry_len;
            if entry_len > 0 {
                if prefix.is_ipv4() {
                    ipv4_counter += 1;
                }
                if prefix.is_ipv6() {
                    ipv6_counter += 1;
                }
            }
        });
        (route_counter, ipv4_counter, ipv6_counter)
    }

    pub fn export_to_file(&self, file_path: &str) {
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(file_path).unwrap();
        let writer = BufWriter::new(file);
        let routes: Vec<(String, BgpRibEntry)> = self
            .treebitmap
            .iter()
            .map(|(network, value)| (network.to_string(), value.clone()))
            .collect();
        serde_json::to_writer_pretty(writer, &routes).unwrap();
    }

    pub fn import_from_file(file_path: &str) -> Self {
        use std::fs::File;
        use std::io::BufReader;

        let file: File = File::open(file_path).unwrap();
        let reader = BufReader::new(file);
        let routes: Vec<(String, BgpRibEntry)> = serde_json::from_reader(reader).unwrap();

        let mut rib = Self::new();
        for (prefix, route) in routes {
            if let Ok(network) = IpNetwork::from_str(&prefix) {
                rib.treebitmap.insert(network, route);
            }
        }
        return rib;
    }

    pub fn register_router(&mut self, router_id: &Uuid) {
        self.router_mask_map.get(router_id);
    }

    #[cfg(test)] // Router mask is private, this is only for testing
    pub fn get_router_mask_map(&self) -> &RouterMaskMap {
        &self.router_mask_map
    }

    pub fn get_routes_for_router(&self, address: IpAddr, router: &Uuid) -> Vec<&Route> {
        let router_mask = self.router_mask_map.try_get(router);
        self.treebitmap
            .longest_match(address)
            .map(|(_prefix, entry)| entry.get_with_mask(router_mask))
            .unwrap_or_default()
    }

    pub fn get_bestroute_for_router(&self, address: IpAddr, router: &Uuid) -> Option<BestRoute> {
        let routes = self.get_routes_for_router(address, router);
        BestRoute::calculate(routes)
    }

    pub fn insert_route(&mut self, route: &Route, id: &Uuid) -> Option<BestRoute> {
        let router_mask = self.router_mask_map.get(id);
        let mut bestroute = None;
        match self.treebitmap.exact_match_mut(route.prefix) {
            Some(entry) => {
                // Entry exists, insert route and check if it is a new bestroute
                // TODO: Adjust this when the rib entry is sorted
                let mut all_routes = entry.get_all();
                all_routes.push(route);
                if BestRoute::calculate(all_routes).unwrap() == BestRoute::from(route.clone()) {
                    bestroute = Some(BestRoute::from(route.clone()));
                }
                entry.insert(router_mask.clone(), route.clone());
            }
            None => {
                // No entry exists, create one and return new bestroute
                bestroute = Some(BestRoute::from(route.clone()));
                let mut rib_entry = BgpRibEntry::new();
                rib_entry.insert(router_mask.clone(), route.clone());
                self.treebitmap.insert(route.prefix, rib_entry);
            }
        }
        return bestroute;
    }

    pub fn remove_route(&mut self, route: &Route, id: &Uuid) -> Option<BestRoute> {
        let router_mask = self.router_mask_map.get(id);
        if let Some(entry) = self.treebitmap.exact_match_mut(route.prefix) {
            let prev_bestroute = BestRoute::calculate(entry.get_all());
            entry.withdraw(router_mask, route);
            if entry.len() == 0 {
                self.treebitmap.remove(route.prefix.clone());
            }
            if let Some(prev_bestroute) = prev_bestroute {
                if prev_bestroute == BestRoute::from(route.clone()) {
                    return Some(prev_bestroute);
                }
            }
        }
        None
    }
}
impl Clone for BgpRib {
    fn clone(&self) -> Self {
        let mut new_rib = BgpRib::new();
        // Clone the treebitmap iteratively
        for (network, entry) in self.treebitmap.iter() {
            new_rib.treebitmap.insert(network, entry.clone());
        }
        new_rib.router_mask_map = self.router_mask_map.clone();
        // new_rib.best_routes = self.best_routes.clone();
        return new_rib;
    }
}

impl PartialEq for BgpRib {
    fn eq(&self, other: &Self) -> bool {
        let routes1: Vec<_> = self.treebitmap.iter().collect();
        let routes2: Vec<_> = other.treebitmap.iter().collect();
        return routes1 == routes2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::route::PathElement;
    use crate::utils::router_mask::RouterMask;
    
    fn test_create_rib_with_routes() -> BgpRib {
        let mut rib = BgpRib::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        rib.insert_route(
            &Route {
                prefix: IpNetwork::from_str_truncate("1.1.1.1/32").unwrap(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![
                    PathElement::ASN(1),
                    PathElement::ASN(2),
                    PathElement::ASN(3),
                ],
                ..Default::default()
            },
            &id1,
        );
        rib.insert_route(
            &Route {
                prefix: IpNetwork::from_str_truncate("2.2.0.0/8").unwrap(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![PathElement::ASN(1), PathElement::ASN(4)],
                ..Default::default()
            },
            &id1,
        );
        rib.insert_route(
            &Route {
                prefix: IpNetwork::from_str_truncate("2.2.0.0/8").unwrap(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![PathElement::ASN(1), PathElement::ASN(40)],
                ..Default::default()
            },
            &id2,
        );
        rib.insert_route(
            &Route {
                prefix: IpNetwork::from_str_truncate("2.2.1.3/32").unwrap(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![PathElement::ASN(1), PathElement::ASN(4)],
                ..Default::default()
            },
            &id1,
        );
        rib.insert_route(
            &Route {
                prefix: IpNetwork::from_str_truncate("2.2.1.3/32").unwrap(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![PathElement::ASN(1), PathElement::ASN(4)],
                ..Default::default()
            },
            &id2,
        );
        return rib;
    }

    #[test]
    fn test_new() {
        let rib = BgpRib::new();
        assert!(rib.treebitmap.is_empty());
    }

    #[test]
    fn test_clone() {
        let rib = test_create_rib_with_routes();
        let rib_clone = rib.clone();
        assert!(rib == rib_clone);
    }

    #[test]
    fn test_export_import() {
        use std::fs::remove_file;
        let file_path = "test.json";

        let rib1 = test_create_rib_with_routes();

        rib1.export_to_file(file_path);
        let rib2 = BgpRib::import_from_file(file_path);
        remove_file(file_path).unwrap();

        assert!(rib1 == rib2);
    }

    #[test]
    fn test_get_routes_for_router() {
        let mut rib = BgpRib::new();
        let id = Uuid::new_v4();
        let route_insert = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ],
            ..Default::default()
        };
        rib.insert_route(&route_insert, &id);

        let route_match = rib.get_routes_for_router(IpAddr::from_str("1.1.1.1").unwrap(), &id);
        assert_eq!(route_match.len(), 1);
        assert_eq!(route_match[0], &route_insert);
    }

    #[test]
    fn test_treebitmap_entry() {
        let mut entry = BgpRibEntry::new();
        assert_eq!(entry.len(), 0);

        let mask1 = RouterMask(0b001);
        let route1 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ],
            ..Default::default()
        };
        let mask2 = RouterMask(0b010);
        let route2 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.10.1".to_string(),
            as_path: vec![PathElement::ASN(5), PathElement::ASN(3)],
            ..Default::default()
        };
        let mask3 = mask2.clone();
        let route3 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.20.1".to_string(),
            as_path: vec![
                PathElement::ASN(15),
                PathElement::ASN(31),
                PathElement::ASN(3),
            ],
            ..Default::default()
        };
        let mask4 = RouterMask(0b100);
        let route4 = route1.clone();

        entry.insert(mask1.clone(), route1.clone());
        entry.insert(mask2.clone(), route2.clone());
        entry.insert(mask3.clone(), route3.clone());
        entry.insert(mask4.clone(), route4.clone());
        assert_eq!(entry.len(), 3); // route 1 and 4 are stored in the same entry

        assert_eq!(
            entry.get_with_mask(&RouterMask::default()),
            Vec::<&Route>::new()
        );
        assert_eq!(entry.get_with_mask(&mask1), vec![&route1]);
        assert_eq!(entry.get_with_mask(&mask2), vec![&route2, &route3]);
        assert_eq!(
            entry.get_with_mask(&mask1.combine(&mask2)),
            Vec::<&Route>::new()
        );
        assert_eq!(entry.get_all(), vec![&route1, &route2, &route3]);
        assert_eq!(entry.get_with_mask(&mask1.combine(&mask4)), vec![&route1]);
    }

    #[test]
    fn test_withdraw() {
        let mut entry = BgpRibEntry::new();

        // Create test routes and masks
        let mask1 = RouterMask(0b001);
        let mask2 = RouterMask(0b010);
        let mask3 = RouterMask(0b100);

        let route1 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.1".to_string(),
            peer: IpAddr::from_str("192.168.1.1").unwrap(),
            as_path: vec![PathElement::ASN(1), PathElement::ASN(2)],
            ..Default::default()
        };

        let route2 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.2".to_string(),
            peer: IpAddr::from_str("192.168.1.2").unwrap(),
            as_path: vec![PathElement::ASN(3), PathElement::ASN(4)],
            ..Default::default()
        };

        // Insert routes
        entry.insert(mask1.clone(), route1.clone());
        entry.insert(mask2.clone(), route1.clone());
        entry.insert(mask3.clone(), route2.clone());
        assert_eq!(entry.len(), 2);

        // Test 1: Remove mask from matching routes (but different masks)
        entry.withdraw(&mask1, &route1);
        assert_eq!(entry.len(), 2);
        assert_eq!(entry.get_with_mask(&mask1), Vec::<&Route>::new());
        assert_eq!(entry.get_with_mask(&mask2), vec![&route1]);
        assert_eq!(entry.get_with_mask(&mask3), vec![&route2]);

        // Test 2: Withdraw exact match
        entry.withdraw(&mask2, &route1);
        assert_eq!(entry.len(), 1);
        assert_eq!(entry.get_with_mask(&mask2), Vec::<&Route>::new());
        assert_eq!(entry.get_with_mask(&mask3), vec![&route2]);

        // Test 3: Withdraw non-existent route
        entry.withdraw(&mask1, &route1);
        assert_eq!(entry.len(), 1);
        assert_eq!(entry.get_with_mask(&mask3), vec![&route2]);
    }

    #[test]
    fn update_bestroute() {
        let mut rib = BgpRib::new();
        let id = Uuid::new_v4();

        // Insert initial route with longer AS path
        let route1 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ],
            peer: IpAddr::from_str("192.168.1.1").unwrap(),
            ..Default::default()
        };

        // First route should become best route
        let bestroute1 = rib.insert_route(&route1, &id);
        assert!(bestroute1.is_some());
        assert_eq!(bestroute1.unwrap().0, route1.clone());

        // Insert better route (shorter AS path)
        let route2 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.2".to_string(),
            as_path: vec![PathElement::ASN(1), PathElement::ASN(3)],
            peer: IpAddr::from_str("192.168.1.2").unwrap(),
            ..Default::default()
        };

        // Second route should become new best route due to shorter AS path
        let bestroute2 = rib.insert_route(&route2, &id);
        assert!(bestroute2.is_some());
        assert_eq!(bestroute2.unwrap().0, route2.clone());
    }

    #[test]
    fn withdraw_bestroute() {
        let mut rib = BgpRib::new();
        let id = Uuid::new_v4();

        // Insert two routes for same prefix
        let route1 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ],
            peer: IpAddr::from_str("192.168.1.1").unwrap(),
            ..Default::default()
        };

        let route2 = Route {
            prefix: IpNetwork::from_str_truncate("1.1.1.0/24").unwrap(),
            next_hop: "192.168.1.2".to_string(),
            as_path: vec![PathElement::ASN(1), PathElement::ASN(3)],
            peer: IpAddr::from_str("192.168.1.2").unwrap(),
            ..Default::default()
        };

        rib.insert_route(&route1, &id);
        rib.insert_route(&route2, &id);

        // Assert that route2 is the best route
        let bestroute2 = rib.get_bestroute_for_router(IpAddr::from_str("1.1.1.1").unwrap(), &id);
        assert!(bestroute2.is_some());
        assert_eq!(bestroute2.unwrap().0, route2.clone());

        // Withdraw the best route (route2 - shorter AS path)
        let withdrawn = rib.remove_route(&route2, &id);
        assert!(withdrawn.is_some());
        assert_eq!(withdrawn.unwrap().0, route2.clone());

        // Verify route1 is now the best route
        let current_best = rib.get_bestroute_for_router(IpAddr::from_str("1.1.1.1").unwrap(), &id);
        assert!(current_best.is_some());
        assert_eq!(current_best.unwrap().0, route1.clone());
    }
}
