use crate::components::advertisement::Advertisement;
use crate::components::bgp::bgp_session::BgpSession;
use crate::components::filters::{Filter, NoFilter};
use crate::modules::router::RouterError;
use crate::utils::message_bus::{MessageReceiver, MessageSender};
use crate::utils::mutex_utils::TryLockWithTimeout;
use std::error::Error;
use std::mem::replace;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

const INCOMING_ADVERTISEMENTS_CAPACITY: usize = 1000;
const OUTGOING_ADVERTISEMENTS_CAPACITY: usize = 1000;

pub struct Interface {
    pub id: Uuid,
    ip_address: IpAddr,
    bgp_session: Option<BgpSession>,
    in_channel: Option<Arc<Mutex<MessageReceiver>>>,
    out_channel: Option<MessageSender>,
    in_filter: Box<dyn Filter<Advertisement>>,
    out_filter: Box<dyn Filter<Advertisement>>,
    incoming_advertisements: Vec<Advertisement>,
    outgoing_advertisements: Vec<Advertisement>,
}

impl Interface {
    pub fn new(ip_address: IpAddr) -> Self {
        Self {
            id: Uuid::new_v4(),
            ip_address,
            bgp_session: None,
            in_channel: None,
            out_channel: None,
            in_filter: Box::new(NoFilter),
            out_filter: Box::new(NoFilter),
            incoming_advertisements: Vec::with_capacity(INCOMING_ADVERTISEMENTS_CAPACITY),
            outgoing_advertisements: Vec::with_capacity(OUTGOING_ADVERTISEMENTS_CAPACITY),
        }
    }

    pub fn get_ip_address(&self) -> IpAddr {
        self.ip_address
    }

    pub fn set_ip_address(&mut self, address: IpAddr) {
        self.ip_address = address;
    }

    pub fn get_bgp_session(&self) -> Option<&BgpSession> {
        self.bgp_session.as_ref()
    }

    pub fn set_bgp_session(&mut self, bgp_session: BgpSession) -> Result<(), Box<dyn Error>> {
        if self.bgp_session.is_some() {
            return Err("Interface already has a BGP session!".into());
        }
        self.bgp_session = Some(bgp_session);
        Ok(())
    }

    pub fn receive(&mut self) -> Result<(), RouterError> {
        match &self.in_channel {
            None => {
                return Ok(());
            } // If this interface is send only, return Ok
            Some(mutex) => {
                let channel = mutex.try_lock_with_timeout(std::time::Duration::from_millis(100))?;
                while let Ok(message) = channel.try_recv() {
                    let mut ad = message
                        .cast::<Advertisement>()
                        .ok_or(RouterError::InterfaceError(
                            "Received message is not an advertisement!".to_string(),
                        ))?
                        .clone();
                    if self.in_filter.filter(&mut ad) {
                        if self.incoming_advertisements.len()
                            < self.incoming_advertisements.capacity()
                        {
                            self.incoming_advertisements.push(ad);
                        } else {
                            return Err(RouterError::InterfaceError(
                                "Received advertisements capacity exceeded!".to_string(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn send(&mut self) -> Result<(), RouterError> {
        match &mut self.out_channel {
            None => (),
            Some(channel) => {
                for mut ad in self.outgoing_advertisements.drain(..) {
                    if self.out_filter.filter(&mut ad) {
                        channel
                            .try_send(Box::new(ad))
                            .map_err(|e| RouterError::InterfaceError(e.to_string()))?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_incoming_advertisements(&mut self) -> Vec<Advertisement> {
        replace(
            &mut self.incoming_advertisements,
            Vec::with_capacity(INCOMING_ADVERTISEMENTS_CAPACITY),
        )
    }

    pub fn push_outgoing_advertisements(
        &mut self,
        mut advertisements: Vec<Advertisement>,
    ) -> Result<(), RouterError> {
        let capacity_left = std::cmp::min(
            advertisements.len(),
            self.outgoing_advertisements.capacity() - self.outgoing_advertisements.len(),
        );
        if capacity_left <= 0 {
            return Err(RouterError::InterfaceError(
                "Outgoing advertisements capacity exceeded!".to_string(),
            ));
        }
        self.outgoing_advertisements
            .extend(advertisements.drain(..capacity_left));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::bgp::bgp_config::SessionConfig;
    use crate::components::bgp::bgp_session::SessionType;
    use crate::utils::message_bus::MessageBus;
    use std::net::Ipv4Addr;
    use std::vec;

    #[test]
    fn test_interface_creation() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let interface = Interface::new(ip);

        assert_eq!(interface.get_ip_address(), ip);
        assert!(interface.get_bgp_session().is_none());
        assert!(interface.incoming_advertisements.is_empty());
        assert!(interface.outgoing_advertisements.is_empty());
    }

    #[test]
    fn test_set_ip_address() {
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let new_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        interface.set_ip_address(new_ip);
        assert_eq!(interface.get_ip_address(), new_ip);
    }

    #[test]
    fn test_bgp_session_management() {
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let config = SessionConfig {
            session_type: SessionType::IBgp,
            as_number: 65000,
        };
        let bgp_session = BgpSession::new(config);

        // Test setting BGP session
        assert!(interface.set_bgp_session(bgp_session).is_ok());
        assert!(interface.get_bgp_session().is_some());

        // Test setting second BGP session (should fail)
        let second_config = SessionConfig {
            session_type: SessionType::EBgp,
            as_number: 65001,
        };
        let second_session = BgpSession::new(second_config);
        assert!(interface.set_bgp_session(second_session).is_err());
    }

    #[test]
    fn test_message_handling() {
        let mut message_bus = MessageBus::new();
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        // Set up channels
        let channel_id = message_bus.create_channel(1).unwrap();
        let sender = message_bus.publish(channel_id).unwrap();
        interface.in_channel = Some(Arc::new(Mutex::new(
            message_bus.subscribe(channel_id).unwrap(),
        )));

        let channel_id = message_bus.create_channel(1).unwrap();
        interface.out_channel = Some(message_bus.publish(channel_id).unwrap());
        let receiver = message_bus.subscribe(channel_id).unwrap();

        // Test receiving messages
        let ad = Advertisement {
            raw: Some("test advertisement".to_string()),
            ..Default::default()
        };
        sender.send(Box::new(ad.clone())).unwrap();

        assert!(interface.receive().is_ok());
        let received_ads = interface.get_incoming_advertisements();
        assert_eq!(received_ads.len(), 1);
        assert_eq!(received_ads[0], ad);

        // Test sending messages
        let ads_to_send = vec![ad.clone()];
        assert!(interface.push_outgoing_advertisements(ads_to_send).is_ok());
        assert!(interface.send().is_ok());

        let received = receiver.try_recv().unwrap();
        let received_ad = received.cast::<Advertisement>().unwrap();
        assert_eq!(*received_ad, ad);
    }

    #[test]
    fn test_capacity_limits() {
        let mut message_bus = MessageBus::new();
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        // Set up channels
        let channel_id = message_bus
            .create_channel(INCOMING_ADVERTISEMENTS_CAPACITY + 1)
            .unwrap();
        let sender = message_bus.publish(channel_id).unwrap();
        interface.in_channel = Some(Arc::new(Mutex::new(
            message_bus.subscribe(channel_id).unwrap(),
        )));

        let channel_id = message_bus
            .create_channel(OUTGOING_ADVERTISEMENTS_CAPACITY + 1)
            .unwrap();
        interface.out_channel = Some(message_bus.publish(channel_id).unwrap());
        let receiver = message_bus.subscribe(channel_id).unwrap();

        // Generate new advertisements
        fn get_ad() -> Advertisement {
            Advertisement {
                timestamp: std::time::Instant::now().elapsed().as_secs_f64(),
                raw: Some("test advertisement".to_string()),
                ..Default::default()
            }
        }
        fn get_ad_boxed() -> Box<Advertisement> {
            Box::new(get_ad())
        }

        // Test incoming capacity limit
        for _ in 0..INCOMING_ADVERTISEMENTS_CAPACITY {
            sender.send(get_ad_boxed()).unwrap();
        }
        assert!(interface.receive().is_ok());

        // Try to exceed capacity
        sender.send(get_ad_boxed()).unwrap();
        assert!(interface.receive().is_err());

        // Test if we can receive again after the buffer was cleared
        assert!(interface.get_incoming_advertisements().len() == INCOMING_ADVERTISEMENTS_CAPACITY);
        assert!(interface.receive().is_ok());

        // Test outgoing capacity limit
        let mut ads = Vec::with_capacity(OUTGOING_ADVERTISEMENTS_CAPACITY);
        for _ in 0..OUTGOING_ADVERTISEMENTS_CAPACITY {
            ads.push(get_ad());
        }
        assert!(interface.push_outgoing_advertisements(ads).is_ok());

        // Try to exceed capacity
        assert!(interface
            .push_outgoing_advertisements(vec![get_ad()])
            .is_err());

        // Test if we can send again after the buffer was cleared
        assert!(interface.send().is_ok());
        assert!(interface
            .push_outgoing_advertisements(vec![get_ad()])
            .is_ok());
    }
}
