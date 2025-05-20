use crate::components::advertisement::Advertisement;
use crate::components::bgp::bgp_bestroute::BestRoute;
use crate::components::bgp::bgp_process::BgpProcess;
use crate::components::bgp::bgp_rib::BgpRib;
use crate::components::interface::Interface;
use crate::utils::message_bus::MessageBusError;
use crate::utils::message_bus::{MessageReceiver, MessageSender};
use crate::utils::mutex_utils::TryLockWithTimeout;
use crate::utils::state_machine::{State, StateTransition};
use log::{error, warn};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};
use thiserror::Error;
use uuid::Uuid;

pub enum RouterChannel {
    Inbound(MessageReceiver),
    Outbound(MessageSender),
}

#[derive(Clone, PartialEq, Eq)]
pub struct RouterOptions {}
impl Default for RouterOptions {
    fn default() -> Self {
        Self {}
    }
}

pub struct Router {
    pub id: Uuid,
    pub options: RouterOptions,
    bgp_processes: Vec<BgpProcess>,
}

impl Router {
    pub fn new(id: Uuid) -> Self {
        let options = RouterOptions::default();
        Router {
            id,
            bgp_processes: Vec::new(),
            options,
        }
    }

    pub fn set_options(&mut self, options: RouterOptions) {
        self.options = options;
    }

    pub fn new_with_options(id: Uuid, options: RouterOptions) -> Self {
        let mut router = Self::new(id);
        router.set_options(options);
        return router;
    }

    pub fn add_bgp_process(&mut self, mut process: BgpProcess) {
        process.set_router_id(&self.id);
        self.bgp_processes.push(process);
    }

    pub fn with_bgp_process(mut self, process: BgpProcess) -> Self {
        self.add_bgp_process(process);
        return self;
    }

    pub fn get_bgp_processes(&self) -> &Vec<BgpProcess> {
        &self.bgp_processes
    }

    pub fn get_bgp_processes_mut(&mut self) -> &mut Vec<BgpProcess> {
        &mut self.bgp_processes
    }
}

impl State for Router {
    fn work(&mut self) -> StateTransition {
        for process in &mut self.bgp_processes {
            process.receive();
            process.get_best_route_changes();
            process.generate_advertisement();
            // TODO: At this point we can share advertisements between different processes
            process.send();
        }
        StateTransition::Continue
    }
}

#[derive(Error, Debug)]
pub enum RouterError {
    #[error("Message bus error: {0}")]
    MessageBusError(String),
    #[error("Failed to lock resource: {0}")]
    LockError(String),
    #[error("Failed to process message: {0}")]
    ProcessError(String),
    #[error("RIB is not set for router {0}")]
    RibNotSet(String),
    #[error("Interface error: {0}")]
    InterfaceError(String),
}

impl From<MessageBusError> for RouterError {
    fn from(err: MessageBusError) -> Self {
        RouterError::MessageBusError(err.to_string())
    }
}

impl<T> From<PoisonError<T>> for RouterError {
    fn from(err: PoisonError<T>) -> Self {
        RouterError::LockError(err.to_string())
    }
}

impl<T> From<TryLockError<T>> for RouterError {
    fn from(err: TryLockError<T>) -> Self {
        RouterError::LockError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::advertisement::Announcement;
    use crate::components::bgp::bgp_session::BgpSession;
    use crate::components::route::PathElement;
    use crate::utils::state_machine::{StateMachine, StateMachineError};
    use crate::utils::thread_manager::ThreadManager;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_create_router() {
        let router = Router::new(Uuid::new_v4());
        assert!(router.id != Uuid::nil());
    }

    #[test]
    fn test_router_state_machine() -> Result<(), StateMachineError> {
        let mut thread_manager: ThreadManager = ThreadManager::new();
        let router = Router::new(Uuid::new_v4());
        let mut state_machine = StateMachine::new(&mut thread_manager, router)?;
        state_machine.start()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        state_machine.stop()?;
        Ok(())
    }

    #[test]
    fn test_router_add_bgp_process() {
        let mut router = Router::new(Uuid::new_v4()).with_bgp_process(BgpProcess::new());
        router.add_bgp_process(BgpProcess::new());
        assert_eq!(router.bgp_processes.len(), 2);
        assert_eq!(
            router.bgp_processes[0].rib_interface.get_client_id(),
            &router.id
        );
        assert_eq!(
            router.bgp_processes[1].rib_interface.get_client_id(),
            &router.id
        );
    }

    #[test]
    fn test_router_work() {
        let mut thread_manager: ThreadManager = ThreadManager::new();
        let interface = Interface::new_loopback(
            &mut thread_manager,
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            10,
            10,
        );
        let session = BgpSession::new().with_interface(interface);
        let process = BgpProcess::new().with_session(session);
        let mut router = Router::new(Uuid::new_v4()).with_bgp_process(process);

        // Test with no ads
        assert!(matches!(router.work(), StateTransition::Continue));

        // Test with ads
        let ad = Advertisement {
            timestamp: 0.0,
            peer: "192.168.0.1".to_string(),
            path: Some(vec![PathElement::ASN(1)]),
            announcements: Some(vec![Announcement {
                prefixes: vec!["1.0.0.0/8".to_string()],
                next_hop: "192.168.0.1".to_string(),
            }]),
            ..Default::default()
        };
        router.get_bgp_processes_mut()[0].inject_advertisement(ad);
        router.work();  // Send Cycle
        router.work();  // Receive Cycle

        let process = &router.bgp_processes[0];
        let rib = process.rib_interface.get_rib_mutex();
        let rib_lock = rib.lock().unwrap();

        let bestroute = rib_lock
            .get_bestroute_for_router(IpAddr::V4(Ipv4Addr::new(1, 0, 10, 25)), &router.id)
            .unwrap();
        assert_eq!(bestroute.0.next_hop, "192.168.0.1".to_string());
    }
}
