use crate::components::{advertisement::Advertisement, bgp_rib::BgpRib};
use crate::utils::mutex_utils::TryLockWithTimeout;
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

            announcements
                .iter()
                .filter_map(|route| bgp_rib_lock.insert_route(route, &self.router_id));

            withdrawals
                .iter()
                .filter_map(|route| bgp_rib_lock.remove_route(route, &self.router_id));

            drop(bgp_rib_lock);

            // let bestroute_announcements: Vec<BestRoute> = announcements
            //     .iter()
            //     .filter_map(|route| bgp_rib_lock.insert_route(route, &self.router_id))
            //     .collect();

            // let bestroute_withdrawals: Vec<BestRoute> = withdrawals
            //     .iter()
            //     .filter_map(|route| bgp_rib_lock.remove_route(route, &self.router_id))
            //     .collect();
        }
    }
}
