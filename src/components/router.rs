use std::sync::mpsc;
use crate::components::live_bgp_parser::RisLiveMessage;
use log::{error, info, warn};

pub struct Router {
    id: u8,
    processed_messages: u64,
    error_count: u64,
}

impl Router {
    pub fn new(id: u8) -> Self {
        Router {
            id,
            processed_messages: 0,
            error_count: 0,
        }
    }

    fn process_message(&mut self, msg: RisLiveMessage) {
        self.processed_messages += 1;
        println!("Router {}: Processed message {} - Type: {}, Peer: {}", 
            self.id,
            self.processed_messages,
            msg.msg_type,
            msg.data.peer
        );
        
        match msg.data.msg_type.as_str() {
            "UPDATE" => {
                if let Some(announcements) = &msg.data.announcements {
                    for announcement in announcements {
                        info!("Router {}: Received announcement from {} (ASN: {}) - Next hop: {}, Prefixes: {:?}", 
                            self.id,
                            msg.data.peer,
                            msg.data.peer_asn,
                            announcement.next_hop,
                            announcement.prefixes
                        );
                    }
                }
                if let Some(withdrawals) = &msg.data.withdrawals {
                    for prefix in withdrawals {
                        info!("Router {}: Received withdrawal from {} (ASN: {}) - Prefix: {}", 
                            self.id,
                            msg.data.peer,
                            msg.data.peer_asn,
                            prefix
                        );
                    }
                }
            },
            "OPEN" => {
                info!("Router {}: Received OPEN message from {} (ASN: {})", 
                    self.id,
                    msg.data.peer,
                    msg.data.peer_asn
                );
            },
            "KEEPALIVE" => {
                // Silently process keepalives
            },
            _ => {
                warn!("Router {}: Received unknown message type: {} from {} (ASN: {})", 
                    self.id,
                    msg.data.msg_type,
                    msg.data.peer,
                    msg.data.peer_asn
                );
            }
        }

        if self.processed_messages % 1000 == 0 {
            info!("Router {}: Processed {} messages ({} errors)", 
                self.id,
                self.processed_messages,
                self.error_count
            );
        }
    }
}

pub fn start(
    id: u8,
    input_stream: mpsc::Receiver<RisLiveMessage>
) {
    let mut router = Router::new(id);
    info!("Router {} started", id);

    for message in input_stream {
        router.process_message(message);
    }

    info!("Router {} shutting down after processing {} messages ({} errors)", 
        id,
        router.processed_messages,
        router.error_count
    );
}