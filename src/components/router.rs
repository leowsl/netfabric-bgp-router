use std::sync::mpsc;
use crate::components::live_bgp_parser::RisLiveMessage;
use log::{info, warn};

pub struct Router {
    id: u8,
    processed_messages: u64,
}

impl Router {
    pub fn new(id: u8) -> Self {
        Router {
            id,
            processed_messages: 0,
        }
    }

    fn log_status(&self) {
        if self.processed_messages % 1000 == 0 {
            info!("Router {}: Processed {} messages", self.id, self.processed_messages);
        }
    }

    fn process_message(&mut self, msg: RisLiveMessage) {
        self.processed_messages += 1;
        
        let peer_info = format!("{} (ASN: {})", msg.data.peer, msg.data.peer_asn);
        
        match msg.data.msg_type.as_str() {
            "UPDATE" => {
                if let Some(announcements) = msg.data.announcements {
                    for ann in announcements {
                        info!("Router {}: Announcement from {} - Next hop: {}, Prefixes: {:?}", 
                            self.id, peer_info, ann.next_hop, ann.prefixes);
                    }
                }
                if let Some(withdrawals) = msg.data.withdrawals {
                    for prefix in withdrawals {
                        info!("Router {}: Withdrawal from {} - Prefix: {}", 
                            self.id, peer_info, prefix);
                    }
                }
            },
            "OPEN" => info!("Router {}: OPEN from {}", self.id, peer_info),
            "KEEPALIVE" => (),
            _ => warn!("Router {}: Unknown message type: {} from {}", 
                self.id, msg.data.msg_type, peer_info),
        }
    }
}

pub fn start(id: u8, input_stream: mpsc::Receiver<RisLiveMessage>) {
    let mut router = Router::new(id);
    info!("Router {} started", id);

    for message in input_stream {
        router.process_message(message);
        router.log_status();
    }

    info!("Router {} shutting down - processed {} messages", id, router.processed_messages);
}