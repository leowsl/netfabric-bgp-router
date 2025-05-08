use crate::utils::message_bus::{Message, MessageBusError, MessageReceiver, MessageSender};
use crate::utils::thread_manager::{ThreadManager, ThreadManagerError};
use log::{error, info};
use std::thread::sleep;
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

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

pub enum StateTransition {
    Continue,
    Stop,
    Transition(Box<dyn State>),
}

pub trait State: Send + Sync + 'static {
    fn work(&mut self) -> StateTransition;
    fn cleanup(&mut self) {}
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
    fn work(&mut self) -> StateTransition {
        StateTransition::Continue
    }
}

pub struct StateMachine {
    command_sender: MessageSender,
    runner_thread_id: Uuid,
}

impl StateMachine {
    pub fn new<T: State>(
        thread_manager: &mut ThreadManager,
        initial_state: T,
    ) -> Result<Self, StateMachineError> {
        let mut message_bus = thread_manager.lock_message_bus()?;
        let channel_id = message_bus.create_channel(1)?;
        let sender = message_bus.publish(channel_id)?;
        let receiver = message_bus.subscribe(channel_id)?;
        drop(message_bus);

        let state_machine = StateMachine {
            command_sender: sender,
            runner_thread_id: thread_manager
                .start_thread(move || Self::runner(receiver, Box::new(initial_state)))?,
        };

        Ok(state_machine)
    }

    fn runner(state_machine_command_receiver: MessageReceiver, mut state: Box<dyn State>) {
        let mut internal_state = InternalState::Initialized;

        while internal_state != InternalState::Stopped {
            if let Ok(message) = state_machine_command_receiver.try_recv() {
                if let Some(command) = message.cast::<InternalState>() {
                    internal_state = *command;
                } else {
                    error!("Invalid message type. Stopping...");
                    internal_state = InternalState::Stopped;
                }
            }

            match internal_state {
                InternalState::Running => match state.work() {
                    StateTransition::Continue => {}
                    StateTransition::Stop => {
                        info!("State machine done. Stopping...");
                        internal_state = InternalState::Stopped;
                    }
                    StateTransition::Transition(next_state) => {
                        state = next_state;
                    }
                },
                InternalState::Initialized | InternalState::Paused => sleep(Duration::from_secs(1)),
                InternalState::Stopped => state.cleanup(),
            }
        }
    }

    pub fn get_runner_thread_id(&self) -> Uuid {
        self.runner_thread_id
    }

    fn send_command(&self, command: InternalState) -> Result<(), StateMachineError> {
        self.command_sender
            .send(Box::new(command))
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
        self.runner_thread_id
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
        fn work(&mut self) -> StateTransition {
            if self.data > 0 {
                self.data -= 1;
                StateTransition::Continue
            } else {
                info!("Counter is 0. Resetting to 10");
                self.data = 10;
                StateTransition::Continue
            }
        }
    }

    #[test]
    fn create_state_machine() {
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
        state_machine.start()
    }

    #[test]
    fn stop_state_machine() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 10 };
        let mut state_machine = StateMachine::new(&mut thread_manager, start_state)?;
        state_machine.start()?;
        state_machine.stop()
    }

    #[test]
    fn test_state_machine_with_samplestate() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 10 };
        let mut state_machine = StateMachine::new(&mut thread_manager, start_state)?;
        state_machine.start()?;
        state_machine.stop()
    }

    #[test]
    fn test_state_machine_with_samplestate_pause() -> Result<(), StateMachineError> {
        let mut tm = ThreadManager::new();
        let start_state = SampleState { data: 10 };
        let mut state_machine = StateMachine::new(&mut tm, start_state)?;
        let runner_thread_id = state_machine.get_runner_thread_id();

        state_machine.start()?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        state_machine.pause()?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(tm.is_thread_running(&runner_thread_id)?);
        state_machine.resume()?;
        std::thread::sleep(std::time::Duration::from_millis(1000));
        state_machine.stop()?;

        tm.join_thread(&runner_thread_id).unwrap();
        assert!(!tm.is_thread_running(&runner_thread_id)?);

        Ok(())
    }
}
