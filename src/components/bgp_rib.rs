use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr};
use std::net::IpAddr;
use crate::components::bgp::{Route, Advertisement};
use log::info;
use ip_network::IpNetwork;
use ip_network_table::IpNetworkTable;

type RouterMask = u8;
pub struct BgpRib {
    treebitmap: IpNetworkTable<RibEntry>,
    // router_mapping: HashMap<RouterMask, String>,
    best_routes: HashMap<String, HashMap<RouterMask, Route>>,
}

impl BgpRib {
    pub fn new() -> Self {
        Self {
            treebitmap: IpNetworkTable::new(),
            // router_mapping: HashMap::new(),
            best_routes: HashMap::new(),
        }
    }

    pub fn get_prefix_count(&self) -> (usize, usize) {
        self.treebitmap.len()
    }

    pub fn export_to_file(&self, file_path: &str) {   
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(file_path).unwrap();
        let writer = BufWriter::new(file);
        let routes: Vec<(String, RibEntry)> = self
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
        let routes: Vec<(String, RibEntry)> = serde_json::from_reader(reader).unwrap();
        
        let mut rib = Self::new();
        for (prefix, entry) in routes {
            if let Ok(network) = IpNetwork::from_str(&prefix) {
                rib.treebitmap.insert(network, entry);
            }
        }
        return rib;
    }

    pub fn get_routes_for_router(&self, address: IpAddr, router: &RouterMask) -> Vec<Route> {
        return self
            .treebitmap
            .longest_match(address)
            .map(|(_prefix, e)| 
                e.routes.clone()
            )
            .unwrap_or(vec![]);
    }

    pub fn get_bestroute_for_router(&self, prefix: &str, router: &RouterMask) -> Option<Route> {
        self
            .best_routes
            .get(prefix)
            .and_then(|router_map| router_map
                .get(router)
                .cloned()
            )
    }

    pub fn update_route(&mut self, route: Route) {
        let prefix = IpNetwork::from_str(&route.prefix).unwrap();
        let entry = RibEntry {
            data: format!(
                "next_hop: {:?}, as_path: {:?}, community: {:?}",
                route.next_hop, route.as_path, route.community
            ),
            routes: vec![route]
        };
        self.treebitmap.insert(prefix, entry);
    }
}
impl Clone for BgpRib {
    fn clone(&self) -> Self {
        let mut new_rib = BgpRib::new();
        // Clone the treebitmap iteratively
        for (network, entry) in self.treebitmap.iter() {
            new_rib.treebitmap.insert(network, entry.clone());
        }
        new_rib.best_routes = self.best_routes.clone();
        return new_rib;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RibEntry {
    data: String,
    routes: Vec<Route>,
}

impl RibEntry {
    fn new(data: String, routes: Vec<Route>) -> Self {
        Self { 
            data,
            routes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let rib = BgpRib::new();
        assert!(rib.treebitmap.is_empty());
    }

    #[test]
    fn test_clone() {
        let mut rib = BgpRib::new();
        rib.update_route(Route {
            prefix: "1.1.1.1/32".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![1, 2, 3],
            community: vec![vec![1, 2, 3]],
        });
        let rib_clone = rib.clone();

        // Compare tables by converting to sorted vectors
        let routes1: Vec<_> = rib.treebitmap.iter().collect();
        let routes2: Vec<_> = rib_clone.treebitmap.iter().collect();
        assert_eq!(routes1, routes2);
        assert_eq!(rib.best_routes, rib_clone.best_routes);
    }

    #[test]
    fn test_export_import() {
        use std::fs::remove_file;
        let file_path = "test.json";

        let mut rib1 = BgpRib::new();
        rib1.update_route(Route {
            prefix: "1.1.1.1/32".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![1, 2, 3],
            community: vec![vec![1, 2, 3]],
        });
        rib1.update_route(Route {
            prefix: "2.2.0.0/32".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![1, 4],
            community: vec![],
        });

        rib1.export_to_file(file_path);
        let rib2 = BgpRib::import_from_file(file_path);
        
        // Compare tables by converting to sorted vectors
        let routes1: Vec<_> = rib1.treebitmap.iter().collect();
        let routes2: Vec<_> = rib2.treebitmap.iter().collect();
        assert_eq!(routes1, routes2);
        assert_eq!(rib1.best_routes, rib2.best_routes);
        
        remove_file(file_path).unwrap();
    }

    #[test]
    fn test_get_routes_for_router() {
        let mut rib = BgpRib::new();
        let route_insert = Route {
            prefix: "1.1.1.0/24".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![1, 2, 3],
            community: vec![vec![1, 2, 3]],
        };
        rib.update_route(route_insert.clone());

        let route_match = rib.get_routes_for_router(IpAddr::from_str("1.1.1.1").unwrap(), &0);
        assert_eq!(route_match.len(), 1);
        assert_eq!(route_match[0], route_insert);
    }
}
