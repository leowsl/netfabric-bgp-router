mod utils;
mod components;

use components::{live_bgp_parser, router};
use utils::thread_manager::{ThreadManager, Message};
use env_logger;
use log::info;

#[derive(Debug)]
struct StringMessage(String);

impl Message for StringMessage {}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(
        env_logger::Env::default().default_filter_or("info")
    );

    let mut tm: ThreadManager = ThreadManager::new();

    if let Some(id) = tm.message_bus.create_channel(0) {
        for i in 0..10 {
            info!("Sending message {}", i);
            let tx = tm.message_bus.publish(id).unwrap();
            tm.start_thread(move || {
                tx.send(Box::new(StringMessage(format!("Message {}", i)))).unwrap();
            });
        }
        
        let rx = tm.message_bus.subscribe(id).unwrap();
        tm.message_bus.stop(id);
        
        while let Ok(msg) = rx.recv() {
            println!("Received message: {}", msg.cast::<StringMessage>().unwrap().0);
        }

        tm.join_all();        
        info!("Done");
    }

    return;
}
