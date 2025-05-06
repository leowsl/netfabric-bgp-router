use crate::utils::message_bus::{Message, MessageBusError, MessageReceiver, MessageSender};
use crate::utils::thread_manager::{ThreadManagerError, ThreadManager};
use log::{info, error};
use uuid::Uuid;
use std::thread::sleep;
use std::time::Duration;
use thiserror::Error;
use std::marker::PhantomData;



#[derive(Error, Debug)]
pub enum StateMachineError {
    #[error("Thread manager error: {0}")]
    ThreadManagerError(#[from] ThreadManagerError),
    #[error("Message bus error: {0}")]
    MessageBusError(#[from] MessageBusError),
    #[error("State machine error: {0}")]
    StateMachineError(String),
    #[error("No Result was computed.")]
    NoResult(),
}

pub trait State: Send + Sync + 'static + Sized {
    fn work(self) -> Option<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalState {
    Initialized,
    Running,
    Paused,
    Stopped,
}
impl Message for InternalState {}
impl State for InternalState {
    fn work(self) -> Option<Self> { Some(self) }
}

pub struct StateMachine<T: State> {
    command_sender: MessageSender,
    runner_thread_id: Uuid,
    _phantom: PhantomData<T>,   // This is used to avoid the compiler warning about unused type parameters
    // TODO: Add a result field
    // result: Result<Box<dyn State>, StateMachineError>,
}

impl<T: State> StateMachine<T> {
    pub fn new(thread_manager: &mut ThreadManager, state: T) -> Result<Self, StateMachineError> {
        // Create a message bus channel for the state machine
        let mut message_bus = thread_manager.lock_message_bus()?;
        let channel_id = message_bus.create_channel(1)?;
        let sender = message_bus.publish(channel_id)?;
        let receiver = message_bus.subscribe(channel_id)?;
        drop(message_bus);

        // Create the state machine
        let state_machine = StateMachine::<T> {
            command_sender: sender,
            runner_thread_id: thread_manager.start_thread(move || Self::runner(receiver, state))?,
            _phantom: PhantomData,
        };

        Ok(state_machine)
    }

    fn runner(state_machine_command_receiver: MessageReceiver, initial_state: T) {
        let mut internal_state = InternalState::Initialized;
        let mut state: Option<T> = Some(initial_state);

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
                    state = state.unwrap().work();
                    if state.is_none() {
                        info!("State machine is done. Stopping...");
                        internal_state = InternalState::Stopped;
                    }
                }
                InternalState::Initialized | InternalState::Paused => sleep(Duration::from_secs(1)),
                InternalState::Stopped => internal_state = InternalState::Stopped,
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
    
    struct SampleState {
        data: i32,
    }

    impl State for SampleState {
        fn work(mut self) -> Option<Self> {
            if self.data > 0 {
                self.data -= 1;
            } else {
                info!("Counter is 0. Resetting to 10");
                self.data = 10;
            }
            Some(self)
        }
    }

    #[test]
    fn create_state_machine(){
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 10 };
        let state_machine = StateMachine::new(&mut thread_manager, start_state);
        assert!(state_machine.is_ok());
    }

    #[test]
    fn start_state_machine() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 10 };
        let mut state_machine = StateMachine::new(&mut thread_manager, start_state)?;
        return state_machine.start();
    }

    #[test]
    fn stop_state_machine() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 10 };
        let mut state_machine = StateMachine::new(&mut thread_manager, start_state)?;
        state_machine.start()?;
        return state_machine.stop();
    }

    #[test]
    fn test_state_machine_with_samplestate()  -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 10 };
        let mut state_machine = StateMachine::new(&mut thread_manager, start_state)?;
        state_machine.start()?;
        return state_machine.stop();
    }
}
