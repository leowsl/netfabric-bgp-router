use crate::components::advertisement::Advertisement;
use crate::components::interface::Interface;
use crate::components::ris_live_data::RisLiveMessage;
use crate::modules::network::NetworkManagerError;
use crate::modules::router::Router;
use crate::utils::message_bus::Message;
use crate::utils::state_machine::{State, StateTransition};
use crate::utils::thread_manager::ThreadManager;
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use log::{error, info, warn};
use std::net::{IpAddr, Ipv4Addr};
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
        Self::RestartOnError
    }
}

#[derive(Debug, Clone)]
pub struct LiveBgpParserStatistics {
    pub messages_processed: u64,
    pub bytes_received: u64,
    pub errors_encountered: u64,
    pub last_error: Option<String>,
    pub start_time: std::time::Instant,
    pub end_time: std::time::Instant,
    pub last_error_time: Option<std::time::Instant>,
}

impl Default for LiveBgpParserStatistics {
    fn default() -> Self {
        Self {
            messages_processed: 0,
            bytes_received: 0,
            errors_encountered: 0,
            last_error: None,
            last_error_time: None,
            start_time: std::time::Instant::now(),
            end_time: std::time::Instant::now(),
        }
    }
}

pub struct LiveBgpParser {
    url: &'static str,
    stream_handle: Option<JoinHandle<()>>,
    tokio_runtime: Arc<Runtime>,
    byte_stream_receiver: Option<Receiver<Bytes>>,
    bytes_buffer: BytesMut,
    restart_policy: RestartPolicy,
    statistics: LiveBgpParserStatistics,
    interface: Option<Interface>,
}
impl LiveBgpParser {
    pub fn new(interface: Interface) -> Self {
        Self {
            url: RIS_STREAM_URL,
            stream_handle: None,
            tokio_runtime: Arc::new(Runtime::new().unwrap()),
            bytes_buffer: BytesMut::with_capacity(BUFFER_CAPACITY),
            restart_policy: RestartPolicy::default(),
            byte_stream_receiver: None,
            statistics: LiveBgpParserStatistics::default(),
            interface: Some(interface),
        }
    }
    pub fn start_stream(&mut self) -> Result<(), BgpParserError> {
        let url_clone: &'static str = self.url;

        // Channel to handle byte stream
        let (tx, rx) = channel(CHANNEL_CAPACITY);
        self.byte_stream_receiver = Some(rx);

        // Start the stream in tokio runtime because reqwest uses tokio
        self.stream_handle = Some(Arc::clone(&self.tokio_runtime).spawn(async move {
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
        self.byte_stream_receiver = None;
        self.bytes_buffer.clear();
        Ok(())
    }

    pub fn get_statistics(&self) -> &LiveBgpParserStatistics {
        &self.statistics
    }

    pub fn process_buffer(&mut self) -> Result<(), BgpParserError> {
        // Write byte stream to buffer
        let receiver = self
            .byte_stream_receiver
            .as_mut()
            .ok_or_else(|| BgpParserError::StreamError("No receiver found".to_string()))?;
        while let Ok(bytes) = receiver.try_recv() {
            self.bytes_buffer.extend_from_slice(&bytes);
            self.statistics.bytes_received += bytes.len() as u64;
        }

        // Process buffer line by line
        while let Some(pos) = self.bytes_buffer.iter().position(|&b| b == b'\n') {
            let chunk = self.bytes_buffer.split_to(pos + 1);
            let message = serde_json::from_slice::<RisLiveMessage>(&chunk)?;
            if message.msg_type == "ris_message" {
                self.interface
                    .as_mut()
                    .unwrap()
                    .push_outgoing_advertisement(Advertisement::from(message.data))
                    .map_err(|e| BgpParserError::SendError(e.to_string()))?;
                self.statistics.messages_processed += 1;
            }
        }
        Ok(())
    }
}

impl State for LiveBgpParser {
    fn work(&mut self) -> StateTransition {
        if self.interface.is_none() {
            if self.stream_handle.is_some() {
                let _ = self.stop_stream();
            }
            warn!("No interface set for LiveBGP Parser. Stopping State Machine.");
            return StateTransition::Stop;
        }

        // Start stream if not running
        if self.stream_handle.is_none() || self.stream_handle.as_ref().unwrap().is_finished() {
            match self.start_stream() {
                Ok(_) => info!("Starting LiveBGP Stream"),
                Err(e) => {
                    error!("Couldn't start bgp live stream! {}", e);
                    self.statistics.errors_encountered += 1;
                    self.statistics.last_error =
                        Some(format!("Couldn't start bgp live stream! {}", e));
                    self.statistics.last_error_time = Some(std::time::Instant::now());
                    let _ = self.stop_stream(); // could be err as stream was not started
                    match self.restart_policy {
                        RestartPolicy::StopOnError => return StateTransition::Stop,
                        RestartPolicy::RestartOnError => return StateTransition::Continue,
                    }
                }
            }
        }

        // Process buffer and send advertisements
        match self.process_buffer() {
            Ok(_) => { /*info!("Processing Buffer")*/ }
            Err(e) => {
                error!("Couldn't process buffer! {}. Timeout for 1 sec to give routers time to catch up.", e);
                self.statistics.errors_encountered += 1;
                self.statistics.last_error = Some(format!("Couldn't process buffer! {}. Timeout for 1 sec to give routers time to catch up.", e));
                self.statistics.last_error_time = Some(std::time::Instant::now());
                let _ = self.stop_stream();
                match self.restart_policy {
                    RestartPolicy::StopOnError => return StateTransition::Stop,
                    RestartPolicy::RestartOnError => return StateTransition::Continue,
                }
            }
        }

        // Send advertisements
        match self.interface.as_mut().unwrap().send() {
            Ok(_) => { /* info!("Advertisements sent"); */ },
            Err(e) => {
                error!("Couldn't send advertisements! {}", e);
                self.statistics.errors_encountered += 1;
                self.statistics.last_error = Some(format!("Couldn't send advertisements! {}", e));
                self.statistics.last_error_time = Some(std::time::Instant::now());
                let _ = self.stop_stream();
                match self.restart_policy {
                    RestartPolicy::StopOnError => return StateTransition::Stop,
                    RestartPolicy::RestartOnError => return StateTransition::Continue,
                }
            }
        };
        StateTransition::Continue
    }
    fn cleanup(&mut self) {
        self.statistics.end_time = std::time::Instant::now();
        if self.stop_stream().is_err() {
            error!("Couldn't properly cleanup stream!");
        }
    }
}

impl Clone for LiveBgpParser {
    fn clone(&self) -> Self {
        Self {
            url: self.url,
            stream_handle: None,
            tokio_runtime: Arc::new(Runtime::new().unwrap()),
            byte_stream_receiver: None,
            bytes_buffer: self.bytes_buffer.clone(),
            restart_policy: self.restart_policy.clone(),
            statistics: self.statistics.clone(),
            interface: None,
        }
    }
}

pub fn get_parser_with_router(
    thread_manager: &mut ThreadManager,
    link_buffer_size: usize,
) -> Result<(LiveBgpParser, Router), NetworkManagerError> {
    
    // Interfaces
    let mut parser_interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
    let mut router_interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
    
    // Channels
    let (tx, rx) = thread_manager.get_message_bus_channel_pair(link_buffer_size)?;
    parser_interface.set_out_channel(tx);
    router_interface.set_in_channel(rx);

    // Parser & Router
    let parser = LiveBgpParser::new(parser_interface);
    let mut router = Router::new(uuid::Uuid::new_v4());
    router.add_interface(router_interface);

    Ok((parser, router))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::message_bus::MessageReceiverExt;
    use crate::utils::state_machine::StateMachine;
    use crate::utils::thread_manager::ThreadManager;
    use std::net::IpAddr;
    use std::net::Ipv4Addr;

    #[test]
    fn test_live_bgp_parser_initialization() {
        let interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let parser = LiveBgpParser::new(interface);

        assert_eq!(parser.url, RIS_STREAM_URL);
        assert!(parser.stream_handle.is_none());
        assert!(parser.byte_stream_receiver.is_none());
        assert_eq!(parser.restart_policy, RestartPolicy::RestartOnError);
        assert_eq!(parser.statistics.messages_processed, 0);
        assert_eq!(parser.statistics.bytes_received, 0);
        assert_eq!(parser.statistics.errors_encountered, 0);
    }

    #[test]
    fn test_run_parser() {
        let mut thread_manager = ThreadManager::new();

        // Create parser and router
        let (tx, rx) = thread_manager.get_message_bus_channel_pair(1000).unwrap();
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        interface.set_out_channel(tx);
        let parser = LiveBgpParser::new(interface);

        // Create  and start state machines
        let mut parser_sm = StateMachine::new(&mut thread_manager, parser).unwrap();

        parser_sm.start().unwrap();

        // Test if messages are coming in
        assert!(rx.receive_with_retry(10, 100).is_ok());

        parser_sm.stop().unwrap();
    }

    #[test]
    fn test_live_parser_statistics() {
        const TEST_DURATION: std::time::Duration = std::time::Duration::from_secs(1);
        let mut thread_manager = ThreadManager::new();

        // Create parser and router
        let (tx, rx) = thread_manager.get_message_bus_channel_pair(1000).unwrap();
        let mut interface = Interface::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        interface.set_out_channel(tx);
        let parser = LiveBgpParser::new(interface);

        // Create and start parser & Dummy router so that channel does not overflow
        let mut parser_sm = StateMachine::new(&mut thread_manager, parser).unwrap();
        parser_sm.start().unwrap();
        let dummy_router = thread_manager
            .start_thread(move || {
                let start = std::time::Instant::now();
                while start.elapsed() < TEST_DURATION {
                    let _ = rx.try_recv();
                }
            })
            .unwrap();

        std::thread::sleep(TEST_DURATION);

        // Stop and get results
        parser_sm.stop().unwrap();
        let final_state: LiveBgpParser = parser_sm
            .get_final_state_cloned::<LiveBgpParser>(&mut thread_manager)
            .unwrap();
        thread_manager.join_thread::<()>(&dummy_router).unwrap();
        let statistics: &LiveBgpParserStatistics = final_state.get_statistics();
        println!("Statistics: {}", statistics);

        assert!(statistics.messages_processed > 0);
        assert!(statistics.bytes_received > 0);
        assert!(statistics.errors_encountered == 0);
    }

    #[test]
    fn test_parser_with_router() {
        use crate::modules::router::RouterOptions;
        
        let mut thread_manager = ThreadManager::new();
        let (parser, mut router) = get_parser_with_router(&mut thread_manager, 1000).unwrap();

        router.set_options(RouterOptions {
            capacity: 1000,
            use_bgp_rib: false,
            drop_incoming_advertisements: true,
            ..Default::default()
        });

        let mut parser_sm = StateMachine::new(&mut thread_manager, parser).unwrap();
        let mut router_sm = StateMachine::new(&mut thread_manager, router).unwrap();


        router_sm.start().unwrap();
        parser_sm.start().unwrap();

        std::thread::sleep(std::time::Duration::from_secs(2));

        parser_sm.stop().unwrap();
        router_sm.stop().unwrap();

        let final_state: LiveBgpParser = parser_sm
            .get_final_state_cloned::<LiveBgpParser>(&mut thread_manager)
            .unwrap();
        let statistics: &LiveBgpParserStatistics = final_state.get_statistics();
        println!("Statistics: {}", statistics);

        assert!(statistics.messages_processed > 0);
        assert!(statistics.bytes_received > 0);
        assert!(statistics.errors_encountered == 0);
    }
}
