use std::collections::{HashMap, HashSet};

use crate::components::router::Router;
use crate::components::bgp_rib::BgpRib;
use crate::utils::thread_manager::ThreadManager;
use crate::utils::state_machine::{StateMachine, StateMachineError};
use uuid::Uuid;
use log::{info, error};
use std::sync::{Arc, Mutex};

pub struct NetworkManager<'a> {
    rib: Arc<Mutex<BgpRib>>,
    routers: HashMap<Uuid, StateMachine>,
    thread_manager: &'a mut ThreadManager,
}

impl<'a> NetworkManager<'a> {
    pub fn new(thread_manager: &'a mut ThreadManager) -> Self {
        NetworkManager {
            rib: Arc::new(Mutex::new(BgpRib::new())),
            routers: HashMap::new(),
            thread_manager: thread_manager,
        }
    }

    pub fn get_rib_mutex(&self) -> std::sync::MutexGuard<'_, BgpRib> {
        self.rib.lock().unwrap()
    }
    pub fn get_rib(&self) -> BgpRib {
        self.rib.lock().unwrap().clone()
    }

    pub fn create_router(&mut self, id: Uuid) {
        self.insert_router(Router::new(id));
    }

    pub fn insert_router(&mut self, mut router: Router) {
        if self.routers.contains_key(&router.id) {
            panic!("Router already exists");
        }
        else {
            let id: Uuid = router.id.clone();
            router.set_rib(&self.rib);
            if let Ok(state_machine) = StateMachine::new(self.thread_manager, router) {
                self.routers.insert(id, state_machine);
            }
            else {
                panic!("Failed to create state machine for router");
            }
        }
    }

    pub fn start(&mut self) -> Result<(), StateMachineError> {
        info!("Starting network");
        for state_machine in self.routers.values_mut() {
            state_machine.start()?;
        }
        Ok(())
    }
    
    pub fn stop(&mut self) -> Result<(), StateMachineError> {
        info!("Stopping network");
        for state_machine in self.routers.values_mut() {
            state_machine.stop()?;
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    
}
