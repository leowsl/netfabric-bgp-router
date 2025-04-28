use log::info;

use crate::components::live_bgp_parser::RisLiveMessage;
use crate::utils::thread_manager::MessageReceiver;
pub struct Router {
    id: u8,
    receiver: MessageReceiver,
}

impl Router {
    pub fn new(id: u8, receiver: MessageReceiver) -> Self {
        Router {
            id,
            receiver,
        }
    }

    pub fn start(&mut self) {
        info!("Router {} started", self.id);

        while let Ok(msg) = self.receiver.recv() {
            if let Some(bgp_msg) = msg.cast::<RisLiveMessage>() {
                println!("Received BGP message from {} (ASN: {}) - Type: {}", 
                    bgp_msg.data.peer,
                    bgp_msg.data.peer_asn, 
                    bgp_msg.data.msg_type);
            }
        }
    }
}
