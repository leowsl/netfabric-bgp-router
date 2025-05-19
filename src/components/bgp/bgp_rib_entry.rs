use crate::components::route::Route;
use crate::utils::router_mask::RouterMask;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct BgpRibEntry {
    routes: Vec<(RouterMask, Route)>,
}

impl BgpRibEntry {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }
    pub fn insert(&mut self, router: RouterMask, route: Route) {
        if let Some((mask, _)) = self
            .routes
            .iter_mut()
            .find(|(_, other_route)| other_route == &route)
        {
            mask.combine_with(&router);
        } else {
            self.routes.push((router, route));
        }
    }
    pub fn withdraw(&mut self, mask: &RouterMask, route: &Route) {
        self.routes.retain(|(other_mask, other_route)| {
            !(mask == other_mask
                && route.peer == other_route.peer
                && route.prefix == other_route.prefix)
        });
        self.routes
            .iter_mut()
            .for_each(|(other_mask, other_route)| {
                if route.peer == other_route.peer && route.prefix == other_route.prefix {
                    other_mask.remove(mask);
                }
            });
    }
    pub fn get_with_mask(&self, router: &RouterMask) -> Vec<&Route> {
        self.routes
            .iter()
            .filter(|(mask, _)| mask.contains(router))
            .map(|(_, route)| route)
            .collect()
    }
    pub fn get_all(&self) -> Vec<&Route> {
        self.routes.iter().map(|(_, route)| route).collect()
    }
    pub fn len(&self) -> usize {
        self.routes.len()
    }
}