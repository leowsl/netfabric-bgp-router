use crate::components::route::Route;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BestRoute(pub Route);

impl BestRoute {
    pub fn calculate(routes: Vec<&Route>) -> Option<BestRoute> {
        routes
            .into_iter()
            .max()
            .map(|route| BestRoute::from(route.clone()))
    }
}

impl From<Route> for BestRoute {
    fn from(route: Route) -> Self {
        Self(route)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ip_network::IpNetwork;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn route_conversion() {
        let route = Route {
            prefix: IpNetwork::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)), 24).unwrap(),
            next_hop: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)).to_string(),
            ..Default::default()
        };

        let best_route = BestRoute::from(route.clone());
        assert_eq!(best_route.0, route);

        let new_route = Route::from(best_route);
        assert_eq!(new_route, route);
    }
}
