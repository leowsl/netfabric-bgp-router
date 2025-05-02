use netfabric_bgp::utils::thread_manager::ThreadManager;
use netfabric_bgp::utils::message_bus::{Message, MessageSender, MessageReceiver};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq, Clone)]
struct TestMessage(String);
impl Message for TestMessage {}

#[test]
fn test_message_bus() -> Result<(), String> {
    let thread_manager = ThreadManager::new();
    let id = Uuid::new_v4();
    
    let (tx, rx) = if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        message_bus.create_channel_with_uuid(1, id)
            .map_err(|e| format!("Failed to create channel: {:?}", e))?;
            
        let tx = message_bus.publish(id).map_err(|e| format!("Failed to get publisher: {:?}", e))?;
        let rx = message_bus.subscribe(id).map_err(|e| format!("Failed to get subscriber: {:?}", e))?;
        (tx, rx)
    } else {
        return Err("Failed to lock message bus".to_string());
    };

    let sent_msg = TestMessage("Hello, world!".to_string());
    tx.send(Box::new(sent_msg.clone())).map_err(|e| format!("Failed to send message: {:?}", e))?;
    
    let recv_msg = rx.recv().map_err(|e| format!("Failed to receive message: {:?}", e))?;
    let recv_test_msg = recv_msg.cast::<TestMessage>().ok_or("Failed to cast received message")?;
    assert_eq!(recv_test_msg.0, sent_msg.0);
    
    if let Ok(mut message_bus) = thread_manager.lock_message_bus() {
        message_bus.stop(id);
    }
    
    Ok(())
}
