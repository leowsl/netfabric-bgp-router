use ip_network::IpNetwork;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use crate::components::bgp::bgp_bestroute::BestRoute;


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathElement {
    ASN(u64),
    ASSet(Vec<u64>),
}
pub type Path = Vec<PathElement>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub peer: IpAddr,
    pub prefix: IpNetwork,
    pub next_hop: String,
    pub as_path: Path,
}

impl Route {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn is_default(&self) -> bool {
        self == &Route::default()
    }
}

impl Default for Route {
    fn default() -> Self {
        Route {
            peer: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            prefix: IpNetwork::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
            next_hop: "".to_string(),
            as_path: Path::new(),
        }
    }
}

impl From<BestRoute> for Route {
    fn from(best_route: BestRoute) -> Self {
        best_route.0
    }
}