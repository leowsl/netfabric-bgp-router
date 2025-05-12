use crate::components::route::Route;
use crate::utils::router_mask::{RouterMask, RouterMaskMap};
use ip_network::IpNetwork;
use ip_network_table::IpNetworkTable;
use log::error;
use std::net::IpAddr;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct BgpRibTreebitmapEntry(Vec<(RouterMask, Route)>);
impl BgpRibTreebitmapEntry {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn insert(&mut self, router: &RouterMask, route: &Route) {
        if let Some((mask, _)) = self
            .0
            .iter_mut()
            .find(|(_, other_route)| other_route == route)
        {
            mask.combine_with(router);
        } else {
            self.0.push((router.clone(), route.clone()));
        }
    }
    pub fn withdraw(&mut self, router: &RouterMask, route: &Route) {
        self.0
            .retain(|(mask, other_route)| route != other_route || mask.0 & router.0 != router.0);
    }
    pub fn get_with_mask(&self, router: &RouterMask) -> Vec<&Route> {
        self.0
            .iter()
            .filter(|(mask, _)| mask.contains(router))
            .map(|(_, route)| route)
            .collect()
    }
    pub fn get_all(&self) -> Vec<&Route> {
        self.0.iter().map(|(_, route)| route).collect()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}
pub struct BgpRib {
    router_mask_map: RouterMaskMap,
    treebitmap: IpNetworkTable<BgpRibTreebitmapEntry>,
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

    pub fn iter(&self) -> impl Iterator<Item = (IpNetwork, &BgpRibTreebitmapEntry)> {
        self.treebitmap.iter()
    }

    pub fn export_to_file(&self, file_path: &str) {
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(file_path).unwrap();
        let writer = BufWriter::new(file);
        let routes: Vec<(String, BgpRibTreebitmapEntry)> = self
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
        let routes: Vec<(String, BgpRibTreebitmapEntry)> = serde_json::from_reader(reader).unwrap();

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

    // pub fn get_bestroute_for_router(&self, prefix: &str, router: &RouterMask) -> Option<Route> {
    //     self.best_routes
    //         .get(prefix)
    //         .and_then(|router_map| router_map.get(router).cloned())
    // }

    pub fn update_route(&mut self, route: &Route, id: &Uuid) {
        let router_mask = self.router_mask_map.get(id);
        if let Ok(prefix) = IpNetwork::from_str(&route.prefix) {
            if self.treebitmap.exact_match(prefix.clone()).is_none() {
                self.treebitmap.insert(prefix, BgpRibTreebitmapEntry::new());
            }
            self.treebitmap
                .exact_match_mut(prefix)
                .unwrap()
                .insert(router_mask, route);
        } else {
            error!("Invalid prefix: {}", route.prefix);
        }
    }
    pub fn update_routes(&mut self, routes: &Vec<Route>, id: &Uuid) {
        for route in routes {
            self.update_route(route, id);
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

    fn test_create_rib_with_routes() -> BgpRib {
        let mut rib = BgpRib::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        rib.update_route(
            &Route {
                prefix: "1.1.1.1/32".to_string(),
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
        rib.update_route(
            &Route {
                prefix: "2.2.0.0/8".to_string(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![PathElement::ASN(1), PathElement::ASN(4)],
                ..Default::default()
            },
            &id1,
        );
        rib.update_route(
            &Route {
                prefix: "2.2.0.0/8".to_string(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![PathElement::ASN(1), PathElement::ASN(40)],
                ..Default::default()
            },
            &id2,
        );
        rib.update_route(
            &Route {
                prefix: "2.2.1.3/32".to_string(),
                next_hop: "192.168.1.1".to_string(),
                as_path: vec![PathElement::ASN(1), PathElement::ASN(4)],
                ..Default::default()
            },
            &id1,
        );
        rib.update_route(
            &Route {
                prefix: "2.2.1.3/32".to_string(),
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
            prefix: "1.1.1.0/24".to_string(),
            next_hop: "192.168.1.1".to_string(),
            as_path: vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ],
            ..Default::default()
        };
        rib.update_route(&route_insert, &id);

        let route_match = rib.get_routes_for_router(IpAddr::from_str("1.1.1.1").unwrap(), &id);
        assert_eq!(route_match.len(), 1);
        assert_eq!(route_match[0], &route_insert);
    }

    #[test]
    fn test_treebitmap_entry() {
        let mut entry = BgpRibTreebitmapEntry::new();
        assert_eq!(entry.len(), 0);

        let mask1 = RouterMask(0b001);
        let route1 = Route {
            prefix: "1.1.1.0/24".to_string(),
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
            prefix: "1.1.1.0/24".to_string(),
            next_hop: "192.168.10.1".to_string(),
            as_path: vec![PathElement::ASN(5), PathElement::ASN(3)],
            ..Default::default()
        };
        let mask3 = mask2.clone();
        let route3 = Route {
            prefix: "1.1.1.0/24".to_string(),
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

        entry.insert(&mask1, &route1);
        entry.insert(&mask2, &route2);
        entry.insert(&mask3, &route3);
        entry.insert(&mask4, &route4);
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
}
