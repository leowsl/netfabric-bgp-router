use crate::utils::message_bus::{Message, MessageBusError, MessageReceiver, MessageSender};
use crate::utils::thread_manager::{ThreadManager, ThreadManagerError};
use log::{error, info};
use std::thread::sleep;
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;
use std::any::Any;

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

// Trait to allow downcasting of states
pub trait Downcastable: Any + 'static {
    fn as_any(&self) -> &dyn Any;
}
impl<T: Any + 'static> Downcastable for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub trait State: Send + Sync + 'static + Downcastable {
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

    fn runner(state_machine_command_receiver: MessageReceiver, mut state: Box<dyn State>) -> Box<dyn State> {
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
        return state;
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

    pub fn get_runner_thread_id(&self) -> Uuid {
        self.runner_thread_id
    }

    pub fn get_runner_active(&self, thread_manager: &ThreadManager) -> bool {
        thread_manager.is_thread_running(&self.runner_thread_id).unwrap_or(false)
    }

    // every state is downcastable but not necessarily cloneable, so we require it here
    pub fn get_final_state_cloned<T: State + Clone>(&self, thread_manager: &mut ThreadManager) -> Option<T> {
        if let Ok(boxed_state) = thread_manager
            .join_thread::<Box<dyn State>>(&self.runner_thread_id)
            .map_err(StateMachineError::ThreadManagerError) 
        {
            return (*boxed_state).as_any().downcast_ref::<T>().cloned();
        }
        None
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
                info!("Counter is 0. Exiting...");
                StateTransition::Transition(Box::new(ResultState { result: self.data }))
            }
        }
    }
    #[derive(Clone)]
    struct ResultState {
        result: i32,
    }
    impl State for ResultState {
        fn work(&mut self) -> StateTransition {
            StateTransition::Stop
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
    fn test_get_runner_active() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 0 };
        let mut state_machine = StateMachine::new(&mut thread_manager, start_state)?;
        state_machine.start()?;
        assert!(state_machine.get_runner_active(&thread_manager));
        Ok(())
    }

    #[test]
    fn stop_state_machine() -> Result<(), StateMachineError> {
        let mut thread_manager = ThreadManager::new();
        let start_state = SampleState { data: 10000000 };
        let mut state_machine = StateMachine::new(&mut thread_manager, start_state)?;
        state_machine.start()?;
        state_machine.stop()
    }

    #[test]
    fn test_state_machine_with_samplestate_pause() -> Result<(), StateMachineError> {
        let mut tm = ThreadManager::new();
        let start_state = SampleState { data: 10000000 };
        let mut state_machine = StateMachine::new(&mut tm, start_state)?;
        let runner_thread_id = state_machine.get_runner_thread_id();

        state_machine.start()?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        state_machine.pause()?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(tm.is_thread_running(&runner_thread_id)?);
        state_machine.resume()?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        state_machine.stop()?;
        std::thread::sleep(std::time::Duration::from_millis(100));

        assert!(!tm.is_thread_running(&runner_thread_id)?);
        Ok(())
    }

    #[test]
    fn test_state_machine_with_result() -> Result<(), StateMachineError> {
        let mut tm = ThreadManager::new();
        let start_state = SampleState { data: 1000 };
        let mut state_machine = StateMachine::new(&mut tm, start_state)?;
        let runner_thread_id = state_machine.get_runner_thread_id();

        state_machine.start()?;
        assert!(tm.is_thread_running(&runner_thread_id)?);
        
        // Result waits for the state machine to terminate
        let result_state = state_machine.get_final_state_cloned::<ResultState>(&mut tm).unwrap();
        assert_eq!(result_state.result, 0);
        Ok(())
    }
}
