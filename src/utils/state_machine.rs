use crate::utils::message_bus::{Message, MessageBusError, MessageReceiver, MessageSender};
use crate::utils::thread_manager::{ThreadManagerError, ThreadManager};
use log::{info, error};
use uuid::Uuid;
use std::thread::{JoinHandle, sleep};
use std::time::Duration;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalState {
    Initialized,
    Running,
    Paused,
    Stopped,
}
impl Message for InternalState {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateMachineCommand {
    Start,
    Pause,
    Resume,
    Stop,
}


#[derive(Error, Debug)]
pub enum StateMachineError {
    #[error("Thread manager error: {0}")]
    ThreadManagerError(#[from] ThreadManagerError),
    #[error("Message bus error: {0}")]
    MessageBusError(#[from] MessageBusError),
    #[error("State machine error: {0}")]
    StateMachineError(String),
}


pub struct StateMachine {
    command_sender: MessageSender,
    runner_thread_id: Uuid,
}

impl StateMachine {
    pub fn new(thread_manager: &mut ThreadManager) -> Result<Self, StateMachineError> {
        // Create a message bus channel for the state machine
        let mut message_bus = thread_manager.lock_message_bus()?;
        let channel_id = message_bus.create_channel(1)?;
        let sender = message_bus.publish(channel_id)?;
        let receiver = message_bus.subscribe(channel_id)?;
        drop(message_bus);

        // Create the state machine
        let state_machine = StateMachine {
            command_sender: sender,
            runner_thread_id: thread_manager.start_thread(move || Self::runner(receiver))?,
        };

        Ok(state_machine)
    }

    fn runner(state_machine_command_receiver: MessageReceiver) {
        let mut internal_state = InternalState::Initialized;

        // Loop
        while internal_state != InternalState::Stopped {
            // Receive commands
            if let Ok(message) = state_machine_command_receiver.try_recv() {
                if let Some(command) = message.cast::<InternalState>() {
                    internal_state = *command;
                } else {
                    error!("Received an invalid message type. Stopping StateMachine...");
                    internal_state = InternalState::Stopped;
                }
            }

            // Execute logic
            match internal_state {
                InternalState::Running => {
                    info!("State machine is running");
                }
                InternalState::Initialized | InternalState::Paused => {
                    info!("State machine is paused");
                    sleep(Duration::from_secs(1));
                }
                InternalState::Stopped => {
                    info!("Stopping state machine...");
                }
                _ => {
                    error!("State machine is in an invalid state. Stopping...");
                    internal_state = InternalState::Stopped;
                }
            }
        }
    }

    pub fn get_runner_thread_id(&self) -> Uuid {
        self.runner_thread_id
    }

    fn send_command(&self, command: InternalState) -> Result<(), StateMachineError> {
        self.command_sender.send(Box::new(command))
            .map_err(MessageBusError::SendError)?;
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), StateMachineError> {
        self.send_command(InternalState::Running)
    }

    pub fn pause(&mut self) -> Result<(), StateMachineError> {
        self.send_command(InternalState::Paused)
    }

    pub fn resume(&mut self) -> Result<(), StateMachineError> {
        self.send_command(InternalState::Running)
    }

    pub fn stop(&mut self) -> Result<(), StateMachineError> {
        self.send_command(InternalState::Stopped)
    }

    pub fn get_state(&self) -> Uuid {
        return self.runner_thread_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::thread_manager::ThreadManager;
    
    #[test]
    fn create_state_machine(){
        let mut thread_manager = ThreadManager::new();
        let state_machine = StateMachine::new(&mut thread_manager);
        assert!(state_machine.is_ok());
    }

    #[test]
    fn start_state_machine() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let mut state_machine = StateMachine::new(&mut thread_manager)?;
        return state_machine.start();
    }

    #[test]
    fn stop_state_machine() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let mut state_machine = StateMachine::new(&mut thread_manager)?;
        state_machine.start()?;
        return state_machine.stop();
    }
}
