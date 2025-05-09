use crate::components::ris_live_data::RisLiveMessage;
use crate::utils::message_bus::{Message, MessageSender};
use crate::utils::state_machine::{State, StateTransition};
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use log::{error, info};
use std::sync::Arc;
use thiserror::Error;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{channel, Receiver};
use tokio::task::JoinHandle;

static RIS_STREAM_URL: &str =
    "https://ris-live.ripe.net/v1/stream/?format=json&client=Netfabric-Test";

pub const BUFFER_CAPACITY: usize = 10000;
pub const CHANNEL_CAPACITY: usize = 10000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    StopOnError,
    RestartOnError,
}
impl Default for RestartPolicy {
    fn default() -> Self {
        Self::StopOnError
    }
}

// Init State
pub struct LiveBgpParser {
    url: &'static str,
    stream_handle: Option<JoinHandle<()>>,
    runtime: Arc<Runtime>,
    receiver: Option<Receiver<Bytes>>,
    sender: MessageSender,
    bytes_buffer: BytesMut,
    restart_policy: RestartPolicy,
}
impl LiveBgpParser {
    pub fn new(sender: MessageSender) -> Self {
        Self {
            url: RIS_STREAM_URL,
            stream_handle: None,
            runtime: Arc::new(Runtime::new().unwrap()),
            bytes_buffer: BytesMut::with_capacity(BUFFER_CAPACITY),
            restart_policy: RestartPolicy::default(),
            receiver: None,
            sender,
        }
    }
    pub fn start_stream(&mut self) -> Result<(), BgpParserError> {
        let url_clone: &'static str = self.url;

        // Channel to handle byte stream
        let (tx, rx) = channel(CHANNEL_CAPACITY);
        self.receiver = Some(rx);

        // Start the stream in tokio runtime because reqwest uses tokio
        self.stream_handle = Some(Arc::clone(&self.runtime).spawn(async move {
            if let Ok(response) = reqwest::get(url_clone).await {
                let mut stream = response.bytes_stream();
                while let Some(Ok(bytes)) = stream.next().await {
                    // Send bytes to buffer. Blocking if channel is full.
                    if let Err(e) = tx.send(bytes).await {
                        error!(
                            "Couldn't send data from async tokio stream to buffer: {}",
                            e
                        );
                        break;
                    }
                }
            }
        }));
        Ok(())
    }
    pub fn stop_stream(&mut self) -> Result<(), BgpParserError> {
        info!("Stopping BGP Livestream");
        if let Some(handle) = self.stream_handle.take() {
            handle.abort();
        }
        self.receiver = None;
        self.bytes_buffer.clear();
        Ok(())
    }
    pub fn process_buffer(&mut self) -> Result<(), BgpParserError> {
        // Write byte stream to buffer
        let receiver = self
            .receiver
            .as_mut()
            .ok_or_else(|| BgpParserError::StreamError("No receiver found".to_string()))?;
        while let Ok(bytes) = receiver.try_recv() {
            self.bytes_buffer.extend_from_slice(&bytes);
        }

        // Process buffer line by line
        while let Some(pos) = self.bytes_buffer.iter().position(|&b| b == b'\n') {
            let chunk = self.bytes_buffer.split_to(pos + 1);
            if let Ok(message) = serde_json::from_slice::<RisLiveMessage>(&chunk) {
                self.sender.try_send(Box::new(message.data))?;
            }
        }
        Ok(())
    }
}
impl State for LiveBgpParser {
    fn work(&mut self) -> StateTransition {
        // Check if stream is running
        if self.stream_handle.is_none() || self.stream_handle.as_ref().unwrap().is_finished() {
            match self.start_stream() {
                Ok(_) => info!("Starting LiveBGP Stream"),
                Err(e) => {
                    error!("Couldn't start bgp live stream! {}", e);
                    let _ = self.stop_stream(); // could be err as stream was not started
                    match self.restart_policy {
                        RestartPolicy::StopOnError => return StateTransition::Stop,
                        RestartPolicy::RestartOnError => return StateTransition::Continue,
                    }
                }
            }
        }

        // Process buffer
        match self.process_buffer() {
            Ok(_) => { /*info!("Processing Buffer")*/ }
            Err(e) => {
                error!("Couldn't process buffer! {}", e);
                self.stop_stream().unwrap();
                match self.restart_policy {
                    RestartPolicy::StopOnError => return StateTransition::Stop,
                    RestartPolicy::RestartOnError => return StateTransition::Continue,
                }
            }
        }
        StateTransition::Continue
    }
    fn cleanup(&mut self) {
        if self.stop_stream().is_err() {
            error!("Couldn't properly cleanup stream!");
        }
    }
}

// Error definitions
#[derive(Error, Debug, Clone)]
pub enum BgpParserError {
    #[error("Failed to fetch stream: {0}")]
    StreamError(String),
    #[error("Failed to parse message: {0}")]
    ParseError(String),
    #[error("Failed to send message: {0}")]
    SendError(String),
    #[error("Stream ended unexpectedly")]
    StreamEnded,
}
impl From<reqwest::Error> for BgpParserError {
    fn from(err: reqwest::Error) -> Self {
        BgpParserError::StreamError(err.to_string())
    }
}
impl From<serde_json::Error> for BgpParserError {
    fn from(err: serde_json::Error) -> Self {
        BgpParserError::ParseError(err.to_string())
    }
}
impl From<std::sync::mpsc::SendError<Box<dyn Message>>> for BgpParserError {
    fn from(err: std::sync::mpsc::SendError<Box<dyn Message>>) -> Self {
        BgpParserError::SendError(err.to_string())
    }
}

impl From<std::sync::mpsc::TrySendError<Box<dyn Message>>> for BgpParserError {
    fn from(err: std::sync::mpsc::TrySendError<Box<dyn Message>>) -> Self {
        BgpParserError::SendError(err.to_string())
    }
}
