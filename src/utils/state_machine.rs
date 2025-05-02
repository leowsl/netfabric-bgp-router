use crate::utils::message_bus::{Message, MessageBusError, MessageReceiver, MessageSender};
use crate::utils::thread_manager::ThreadManager;
use log::info;

pub enum StateMachineCommand {
    Start,
    Stop,
}
impl Message for StateMachineCommand {}

pub struct StateMachineRunner {
    running: bool,
    receiver: MessageReceiver,
}

pub struct StateMachine {
    sender: MessageSender,
    runner: Option<StateMachineRunner>,
}

impl StateMachineRunner {
    fn new(receiver: MessageReceiver) -> Self {
        Self {
            running: false,
            receiver,
        }
    }

    fn start(&mut self) {
        self.running = true;
        info!("State machine started");
        while self.running {
            // Perform one iteration of the state machine

            // TODO: Implement the state machine logic here
            info!("State machine iteration");
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Check for incoming commands
            if let Ok(message) = self.receiver.try_recv() {
                let command = message.cast::<StateMachineCommand>().unwrap();
                match command {
                    StateMachineCommand::Stop => {
                        self.running = false;
                    }
                    _ => {}
                }
            }
        }
        info!("State machine stopped");
    }
}

impl StateMachine {
    pub fn new(thread_manager: &mut ThreadManager) -> Result<Self, MessageBusError> {
        let message_bus_result = thread_manager.lock_message_bus();
        match message_bus_result {
            Ok(mut message_bus) => {
                let channel_id = message_bus.create_channel(0)?;
                let sender = message_bus.publish(channel_id)?;
                let receiver = message_bus.subscribe(channel_id)?;
                return Ok(StateMachine {
                    sender: sender,
                    runner: Some(StateMachineRunner::new(receiver)),
                });
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn start(&mut self, thread_manager: &mut ThreadManager) {
        if let Some(mut runner) = self.runner.take() {
            let _ = thread_manager.start_thread(move || runner.start());
        }
    }

    pub fn stop(&self) {
        let _ = self.sender.send(Box::new(StateMachineCommand::Stop));
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::thread_manager::ThreadManager;

    #[test]
    fn create_state_machine() {

    }

    fn start_state_machine() {

    }

    fn stop_state_machine() {

    }
}
