use crate::components::route::Route;
use ip_network::IpNetwork;
use ip_network_table::IpNetworkTable;
use log::error;
use uuid::Uuid;
use std::net::IpAddr;
use std::{collections::HashMap, str::FromStr};

#[derive(Debug, PartialEq, Hash, Eq, Copy, Clone)]
pub struct RouterMask(pub u64);
impl Default for RouterMask {
    fn default() -> Self {
        Self(0)
    }
}
impl RouterMask {
    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }
}
pub struct RouterMaskMap {
    default: RouterMask,
    mask: RouterMask,
    map: HashMap<Uuid, RouterMask>,
}
impl RouterMaskMap {
    pub fn new() -> Self {
        Self { default: RouterMask::default(), mask: RouterMask::default(), map: HashMap::new() }
    }
    pub fn try_get(&self, router_id: &Uuid) -> &RouterMask {
        return self.map.get(router_id).unwrap_or(&self.default);
    }
    pub fn get(&mut self, router_id: &Uuid) -> &RouterMask {
        if self.map.contains_key(router_id) {
            return self.map.get(router_id).unwrap();
        }
        if self.mask.0 == u64::MAX {
            panic!("Router mask map is full");
        }
        for bit in 0..64{
            let router_bit = 1 << bit;
            if self.mask.0 & router_bit == 0 {
                self.map.insert(router_id.clone(), RouterMask(router_bit));
                self.mask.0 |= router_bit;
                return self.map.get(router_id).unwrap();
            }
        }
        panic!("RouterMaskMap is corrupted");
    }
    pub fn get_all(&self) -> &RouterMask {
        &self.mask
    }
    pub fn remove(&mut self, router_id: &Uuid) {
        self.mask.0 ^= self.map.remove(router_id).unwrap().0;
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

pub struct BgpRib {
    router_mask_map: RouterMaskMap,
    treebitmap: IpNetworkTable<Route>,
    // router_mapping: HashMap<RouterMask, String>,
    best_routes: HashMap<String, HashMap<RouterMask, Route>>,
}

impl BgpRib {
    pub fn new() -> Self {
        Self {
            router_mask_map: RouterMaskMap::new(),
            treebitmap: IpNetworkTable::new(),
            // router_mapping: HashMap::new(),
            best_routes: HashMap::new(),
        }
    }

    pub fn get_prefix_count(&self) -> (usize, usize) {
        self.treebitmap.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (IpNetwork, &Route)> {
        self.treebitmap.iter()
    }

    pub fn export_to_file(&self, file_path: &str) {
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(file_path).unwrap();
        let writer = BufWriter::new(file);
        let routes: Vec<(String, Route)> = self
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
        let routes: Vec<(String, Route)> = serde_json::from_reader(reader).unwrap();

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

    #[cfg(test)]    // Router mask is private, this is only for testing
    pub fn get_router_mask_map(&self) -> &RouterMaskMap {
        &self.router_mask_map
    }

    //TODO get all routes and not just one
    pub fn get_routes_for_router(&self, address: IpAddr, router: &Uuid) -> Vec<Route> {
        let router_mask = self.router_mask_map.try_get(router);
        return self
            .treebitmap
            .longest_match(address)
            .map(|(_prefix, routes)| vec![routes.clone()])
            .unwrap_or(vec![]);
    }

    pub fn get_bestroute_for_router(&self, prefix: &str, router: &RouterMask) -> Option<Route> {
        self.best_routes
            .get(prefix)
            .and_then(|router_map| router_map.get(router).cloned())
    }

    pub fn update_route(&mut self, route: &Route) {
        if let Ok(prefix) = IpNetwork::from_str(&route.prefix) {
            self.treebitmap.insert(prefix, route.clone());
        }
        else {
            error!("Invalid prefix: {}", route.prefix);
        }
    }
    pub fn update_routes(&mut self, routes: &Vec<Route>) {
        for route in routes {
            self.update_route(route);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::route::PathElement;

    #[test]
    fn test_new() {
        let rib = BgpRib::new();
        assert!(rib.treebitmap.is_empty());
    }

    #[test]
    fn test_clone() {
        let mut rib = BgpRib::new();
        rib.update_route(&Route {
            prefix: "1.1.1.1/32".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ]
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
        rib1.update_route(&Route {
            prefix: "1.1.1.1/32".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ]
        });
        rib1.update_route(&Route {
            prefix: "2.2.0.0/32".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![PathElement::ASN(1), PathElement::ASN(4)]
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
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ]
        };
        rib.update_route(&route_insert);

        let route_match = rib.get_routes_for_router(IpAddr::from_str("1.1.1.1").unwrap(), &Uuid::nil());
        assert_eq!(route_match.len(), 1);
        assert_eq!(route_match[0], route_insert);
    }

    #[test]
    fn test_router_mask_map_create() {
        let router_mask_map = RouterMaskMap::new();
        assert_eq!(router_mask_map.get_all(), &RouterMask(0));
    }

    #[test]
    fn test_router_mask_map_get() {
        let mut router_mask_map = RouterMaskMap::new();
        let r1 = Uuid::new_v4();
        let r2 = Uuid::new_v4();
        let _ = router_mask_map.get(&r1);
        let _ = router_mask_map.get(&r2);

        assert_eq!(router_mask_map.len(), 2);
        assert_eq!(router_mask_map.get(&r1), &RouterMask(0b01));
        assert_eq!(router_mask_map.get(&r2), &RouterMask(0b10));
        assert_eq!(router_mask_map.get_all(), &RouterMask(0b11))
    }

    #[test]
    fn test_router_mask_map_remove() {
        let mut router_mask_map = RouterMaskMap::new();
        let r1 = Uuid::new_v4();
        assert_eq!(router_mask_map.get(&r1), &RouterMask(0b1));
        router_mask_map.remove(&r1);
    }
}
