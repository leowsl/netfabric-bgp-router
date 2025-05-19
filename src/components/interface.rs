use crate::components::advertisement::Advertisement;
use crate::components::filters::{Filter, NoFilter};
use crate::modules::router::RouterError;
use crate::utils::message_bus::{MessageReceiver, MessageSender};
use crate::utils::mutex_utils::TryLockWithTimeout;
use crate::ThreadManager;
use std::mem::replace;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const INCOMING_ADVERTISEMENTS_CAPACITY: usize = 1000;
const OUTGOING_ADVERTISEMENTS_CAPACITY: usize = 1000;

#[derive(Debug)]
pub struct Interface {
    pub id: Uuid,
    ip_address: IpAddr,
    in_channel: Option<Arc<Mutex<MessageReceiver>>>,
    out_channel: Option<MessageSender>,
    in_filter: Box<dyn Filter<Advertisement>>,
    out_filter: Box<dyn Filter<Advertisement>>,
    incoming_advertisements: Vec<Advertisement>,
    outgoing_advertisements: Vec<Advertisement>,
    buffer_size_incoming: usize,
    buffer_size_outgoing: usize,
}

impl Interface {
    pub fn new(ip_address: IpAddr) -> Self {
        Self {
            id: Uuid::new_v4(),
            ip_address,
            in_channel: None,
            out_channel: None,
            in_filter: Box::new(NoFilter),
            out_filter: Box::new(NoFilter),
            incoming_advertisements: Vec::with_capacity(INCOMING_ADVERTISEMENTS_CAPACITY),
            outgoing_advertisements: Vec::with_capacity(OUTGOING_ADVERTISEMENTS_CAPACITY),
            buffer_size_incoming: INCOMING_ADVERTISEMENTS_CAPACITY,
            buffer_size_outgoing: OUTGOING_ADVERTISEMENTS_CAPACITY,
        }
    }

    pub fn new_loopback(
        thread_manager: &mut ThreadManager,
        ip_address: IpAddr,
        queue_buffer_size: usize,
        link_buffer_size: usize,
    ) -> Self {
        let mut interface = Self::new(ip_address);
        let (tx, rx) = thread_manager
            .get_message_bus_channel_pair(link_buffer_size)
            .unwrap();
        interface.set_in_channel(rx);
        interface.set_out_channel(tx);
        interface.set_buffer_size(queue_buffer_size, queue_buffer_size);
        interface
    }

    pub fn get_ip_address(&self) -> IpAddr {
        self.ip_address
    }

    pub fn set_ip_address(&mut self, address: IpAddr) {
        self.ip_address = address;
    }

    pub fn set_in_channel(&mut self, in_channel: MessageReceiver) {
        self.in_channel = Some(Arc::new(Mutex::new(in_channel)));
    }

    pub fn set_out_channel(&mut self, out_channel: MessageSender) {
        self.out_channel = Some(out_channel);
    }

    pub fn set_in_filter<F: Filter<Advertisement>>(&mut self, in_filter: F) {
        self.in_filter = Box::new(in_filter);
    }

    pub fn set_out_filter<F: Filter<Advertisement>>(&mut self, out_filter: F) {
        self.out_filter = Box::new(out_filter);
    }

    pub fn set_buffer_size(&mut self, incoming: usize, outgoing: usize) {
        // New buffer size will be applied at next non-empty send/receive cycle or with force_resize
        self.buffer_size_incoming = incoming;
        self.buffer_size_outgoing = outgoing;
    }

    pub fn force_resize(&mut self) {
        // I mean we could try to shrink or expand.
        // - or just drop the current buffer
        self.incoming_advertisements = Vec::with_capacity(self.buffer_size_incoming);
        self.outgoing_advertisements = Vec::with_capacity(self.buffer_size_outgoing);
    }
    pub fn receive(&mut self) -> Result<(), RouterError> {
        match &self.in_channel {
            None => {
                return Ok(());
            } // If this interface is send only, we just act as if there are no new messages
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
                            // When buffer is full, we stop receiving
                            // log::info!("Received advertisements capacity exceeded!");
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn send(&mut self) {
        if self.outgoing_advertisements.is_empty() {
            return;
        }
        match &mut self.out_channel {
            None => (),
            Some(channel) => {
                // Drain does not seem to work, so we work with std::mem::replace
                for mut ad in replace(
                    &mut self.outgoing_advertisements,
                    Vec::with_capacity(self.buffer_size_outgoing),
                )
                .into_iter()
                {
                    if self.out_filter.filter(&mut ad) {
                        match channel.try_send(Box::new(ad)) {
                            Ok(_) => {}
                            Err(_) => {
                                // log::warn!(e);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn get_incoming_advertisements(&mut self) -> Vec<Advertisement> {
        replace(
            &mut self.incoming_advertisements,
            Vec::with_capacity(self.buffer_size_incoming),
        )
    }

    pub fn push_outgoing_advertisement(
        &mut self,
        advertisement: Advertisement,
    ) -> Result<(), RouterError> {
        if self.out_channel.is_none() {
            // Dont send if theres no out channel
            return Ok(());
        }
        if self.outgoing_advertisements.len() < self.outgoing_advertisements.capacity() {
            self.outgoing_advertisements.push(advertisement);
        } else {
            return Err(RouterError::InterfaceError(
                "Outgoing advertisements buffer capacity exceeded!".to_string(),
            ));
        }
        Ok(())
    }

    pub fn push_outgoing_advertisements(
        &mut self,
        mut advertisements: Vec<Advertisement>,
    ) -> Result<(), RouterError> {
        if advertisements.len() == 0 {
            return Ok(());
        }
        if self.out_channel.is_none() {
            // Dont send if theres no out channel
            return Ok(());
        }
        let capacity_left =
            self.outgoing_advertisements.capacity() - self.outgoing_advertisements.len();
        if advertisements.len() > capacity_left {
            let err: Result<(), RouterError> = Err(RouterError::InterfaceError(format!(
                "Outgoing advertisements buffer capacity exceeded! Dropping {} advertisements!",
                advertisements.len() - capacity_left
            )));
            self.outgoing_advertisements
                .extend(advertisements.drain(..capacity_left));
            return err;
        }
        self.outgoing_advertisements
            .extend(advertisements.drain(..));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::filters::{HostFilter, OwnASFilter};
    use crate::components::route::PathElement;
    use crate::utils::message_bus::MessageBus;
    use crate::utils::thread_manager::ThreadManager;
    use std::net::{IpAddr, Ipv4Addr};
    use std::vec;

    #[test]
    fn test_interface_creation() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let interface = Interface::new(ip);

        assert_eq!(interface.get_ip_address(), ip);
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
        interface.send();

        let received = receiver.try_recv().unwrap();
        let received_ad = received.cast::<Advertisement>().unwrap();
        assert_eq!(*received_ad, ad);
    }

    #[test]
    fn test_filter() {
        let thread_manager = ThreadManager::new();
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        // Filters
        let host_filter = HostFilter::new("good host".to_string());
        let own_asn_filter = OwnASFilter::new(65000);
        interface.set_in_filter(own_asn_filter);
        interface.set_out_filter(host_filter);

        // Channels
        let (tx1, rx1) = thread_manager.get_message_bus_channel_pair(10).unwrap();
        let (tx2, rx2) = thread_manager.get_message_bus_channel_pair(10).unwrap();
        interface.set_in_channel(rx1);
        interface.set_out_channel(tx2);

        // Test outgoing w/ host filter
        // Pass
        let ad1 = Advertisement {
            host: "good host".to_string(),
            path: Some(vec![PathElement::ASN(65000)]),
            raw: Some("test advertisement".to_string()),
            ..Default::default()
        };
        interface.push_outgoing_advertisement(ad1).unwrap();
        interface.send();
        let m = rx2.try_recv();
        assert!(m.is_ok());

        // Fail
        let ad2 = Advertisement {
            host: "bad host".to_string(),
            path: Some(vec![PathElement::ASN(65000)]),
            ..Default::default()
        };
        interface.push_outgoing_advertisement(ad2).unwrap();
        interface.send();
        let m = rx2.try_recv();
        assert!(m.is_err());

        // Test incoming w/ own asn filter
        // Pass
        let ad3 = Advertisement {
            host: "good host".to_string(),
            path: Some(vec![PathElement::ASN(65001)]),
            ..Default::default()
        };
        tx1.send(Box::new(ad3.clone())).unwrap();
        interface.receive().unwrap();
        let received_ads = interface.get_incoming_advertisements();
        assert_eq!(received_ads.len(), 1);
        assert_eq!(received_ads[0], ad3);

        // Fail
        let ad4 = Advertisement {
            host: "good host".to_string(),
            path: Some(vec![PathElement::ASN(65000)]),
            ..Default::default()
        };
        tx1.send(Box::new(ad4.clone())).unwrap();
        interface.receive().unwrap();
        let received_ads = interface.get_incoming_advertisements();
        assert_eq!(received_ads.len(), 0);
    }

    #[test]
    fn test_buffers_capacity() {
        // Create loopback channel
        let mut message_bus = MessageBus::new();
        let channel_id = message_bus.create_channel(100).unwrap();
        let sender = message_bus.publish(channel_id).unwrap();
        let receiver = message_bus.subscribe(channel_id).unwrap();

        // Create interface
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        interface.set_buffer_size(5, 5);
        interface.force_resize();
        assert_eq!(interface.outgoing_advertisements.capacity(), 5);
        assert_eq!(interface.incoming_advertisements.capacity(), 5);
        interface.set_in_channel(receiver);
        interface.set_out_channel(sender);

        // Test outgoing buffer
        assert_eq!(interface.outgoing_advertisements.len(), 0);
        interface
            .push_outgoing_advertisements(vec![Advertisement::default(); 5])
            .unwrap();
        assert_eq!(interface.outgoing_advertisements.len(), 5);
        assert!(interface
            .push_outgoing_advertisement(Advertisement::default())
            .is_err());
        assert_eq!(interface.outgoing_advertisements.len(), 5);
        interface.send();
        assert!(interface
            .push_outgoing_advertisement(Advertisement::default())
            .is_ok());
        assert_eq!(interface.outgoing_advertisements.len(), 1);
        interface.outgoing_advertisements.clear();

        // Test incoming buffer - there are now 5 packets in the queue
        assert_eq!(interface.incoming_advertisements.len(), 0);
        interface.receive().unwrap();
        assert_eq!(interface.incoming_advertisements.len(), 5);

        // We send one more. It should remain in the channel
        interface
            .push_outgoing_advertisement(Advertisement::default())
            .unwrap();
        interface.send();
        assert_eq!(interface.incoming_advertisements.len(), 5);

        drop(interface.get_incoming_advertisements());
        assert_eq!(interface.incoming_advertisements.len(), 0);

        // Now that the buffer is empty, we should be able to receive again
        interface.send();
        assert!(interface.receive().is_ok());
        assert_eq!(interface.incoming_advertisements.len(), 1);
    }
}
