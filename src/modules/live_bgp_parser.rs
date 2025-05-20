use crate::components::advertisement::Advertisement;
use crate::components::bgp::bgp_config::SessionConfig;
use crate::components::bgp::bgp_process::BgpProcess;
use crate::components::bgp::bgp_session::BgpSession;
use crate::components::ris_live_data::RisLiveMessage;
use crate::modules::network::NetworkManagerError;
use crate::modules::router::Router;
use crate::utils::message_bus::Message;
use crate::utils::state_machine::{State, StateTransition};
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use log::{error, info, warn};
use std::sync::Arc;
use thiserror::Error;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{channel, Receiver};
use tokio::task::JoinHandle;

use super::network::NetworkManager;

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
    advertisements: Vec<Advertisement>,
    restart_policy: RestartPolicy,
    statistics: LiveBgpParserStatistics,
    bgp_sessions: Vec<BgpSession>,
}
impl LiveBgpParser {
    pub fn new() -> Self {
        Self {
            url: RIS_STREAM_URL,
            stream_handle: None,
            tokio_runtime: Arc::new(Runtime::new().unwrap()),
            bytes_buffer: BytesMut::with_capacity(BUFFER_CAPACITY),
            advertisements: Vec::new(),
            restart_policy: RestartPolicy::default(),
            byte_stream_receiver: None,
            statistics: LiveBgpParserStatistics::default(),
            bgp_sessions: Vec::new(),
        }
    }

    pub fn add_bgp_session(&mut self, bgp_session: BgpSession) {
        self.bgp_sessions.push(bgp_session);
    }

    pub fn with_bgp_sessions(mut self, bgp_session: BgpSession) -> Self {
        self.add_bgp_session(bgp_session);
        self
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
                self.advertisements.push(Advertisement::from(message.data));
                self.statistics.messages_processed += 1;
            }
        }
        Ok(())
    }
}

impl State for LiveBgpParser {
    fn work(&mut self) -> StateTransition {
        if self.bgp_sessions.is_empty() {
            warn!("No BGP sessions set for LiveBGP Parser.");
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
        };

        // Process buffer and send advertisements
        match self.process_buffer() {
            Ok(_) => {}
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
        };

        // Send advertisements
        for session in &mut self.bgp_sessions {
            let res = session.send(self.advertisements.clone());
            if let Err(e) = res {
                error!("Couldn't send advertisements to BGP session! {:?}", e);
            }
        }
        self.advertisements.clear();

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
            advertisements: self.advertisements.clone(),
            restart_policy: self.restart_policy.clone(),
            statistics: self.statistics.clone(),
            bgp_sessions: Vec::new(),
        }
    }
}

pub fn get_parser_with_bgp_session(
    network_manager: &mut NetworkManager,
) -> Result<(LiveBgpParser, BgpSession), NetworkManagerError> {
    let (session1, session2) = network_manager
        .new_bgp_session_pair(SessionConfig::default(), SessionConfig::default(), 1000)
        .unwrap();

    let parser = LiveBgpParser::new().with_bgp_sessions(session1);

    Ok((parser, session2))
}

pub fn get_parser_with_router(
    network_manager: &mut NetworkManager,
) -> Result<(LiveBgpParser, Router), NetworkManagerError> {
    let (parser, router_session) = get_parser_with_bgp_session(network_manager)?;
    let process = BgpProcess::new().with_session(router_session);
    let router = Router::new(uuid::Uuid::new_v4()).with_bgp_process(process);
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
    #[error("Interface error: {0}")]
    InterfaceError(String),
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

impl From<crate::components::interface::InterfaceError> for BgpParserError {
    fn from(err: crate::components::interface::InterfaceError) -> Self {
        BgpParserError::InterfaceError(format!("{:?}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::bgp::bgp_config::SessionConfig;
    use crate::modules::network::NetworkManager;
    use crate::utils::state_machine::StateMachine;
    use crate::utils::thread_manager::ThreadManager;

    #[test]
    fn test_live_bgp_parser_initialization() {
        let session = BgpSession::new();
        let parser = LiveBgpParser::new().with_bgp_sessions(session);

        assert_eq!(parser.url, RIS_STREAM_URL);
        assert!(parser.stream_handle.is_none());
        assert!(parser.byte_stream_receiver.is_none());
        assert_eq!(parser.restart_policy, RestartPolicy::RestartOnError);
        assert_eq!(parser.bgp_sessions.len(), 1);
        assert_ne!(parser.bgp_sessions[0].id, uuid::Uuid::nil());
        assert_eq!(parser.statistics.messages_processed, 0);
        assert_eq!(parser.statistics.bytes_received, 0);
        assert_eq!(parser.statistics.errors_encountered, 0);
    }

    #[test]
    fn test_run_parser() {
        let mut thread_manager = ThreadManager::new();
        let session = BgpSession::new();
        let parser = LiveBgpParser::new().with_bgp_sessions(session);

        // Create  and start state machines
        let mut parser_sm = StateMachine::new(&mut thread_manager, parser).unwrap();

        // Let run to check its not crashing
        parser_sm.start().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(10));
        parser_sm.stop().unwrap();
    }

    #[test]
    fn test_live_bgp_parser_statistics() {
        const TEST_DURATION: std::time::Duration = std::time::Duration::from_secs(1);

        let mut thread_manager = ThreadManager::new();
        let network = NetworkManager::new(&mut thread_manager);
        let (session1, mut session2) = network
            .new_bgp_session_pair(SessionConfig::default(), SessionConfig::default(), 1000)
            .unwrap();

        let parser = LiveBgpParser::new().with_bgp_sessions(session1);
        let mut parser_sm = StateMachine::new(&mut thread_manager, parser).unwrap();

        // Create and start parser & Dummy router so that channel does not overflow
        parser_sm.start().unwrap();
        let dummy_session = thread_manager
            .start_thread(move || {
                let start = std::time::Instant::now();
                while start.elapsed() < TEST_DURATION {
                    let _ = session2.receive();
                }
            })
            .unwrap();

        std::thread::sleep(TEST_DURATION);

        // Stop and get results
        parser_sm.stop().unwrap();
        let final_state: LiveBgpParser = parser_sm
            .get_final_state_cloned::<LiveBgpParser>(&mut thread_manager)
            .unwrap();
        thread_manager.join_thread::<()>(&dummy_session).unwrap();

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
        let mut network_manager = NetworkManager::new(&mut thread_manager);
        let (parser, mut router) = get_parser_with_router(&mut network_manager).unwrap();

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

    #[test]
    fn test_parser_with_multiple_sessions() {
        const TEST_DURATION: std::time::Duration = std::time::Duration::from_secs(1);

        // Create network with 2 sessions
        let mut thread_manager = ThreadManager::new();
        let network = NetworkManager::new(&mut thread_manager);
        let (session1, mut session2) = network
            .new_bgp_session_pair(SessionConfig::default(), SessionConfig::default(), 1000)
            .unwrap();
        let (session3, mut session4) = network
            .new_bgp_session_pair(SessionConfig::default(), SessionConfig::default(), 1000)
            .unwrap();

        // Create and start parser
        let parser = LiveBgpParser::new()
            .with_bgp_sessions(session1)
            .with_bgp_sessions(session3);
        let mut parser_sm = StateMachine::new(&mut thread_manager, parser).unwrap();
        parser_sm.start().unwrap();

        // Count how many messages are received for each session
        let dummy_session = thread_manager
            .start_thread(move || {
                let start = std::time::Instant::now();
                let mut count2 = 0;
                let mut count4 = 0;
                // Add 100ms to ensure all messages are received before the test ends
                while start.elapsed() < TEST_DURATION + std::time::Duration::from_millis(100) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    let one = session2.receive().unwrap();
                    let two = session4.receive().unwrap();
                    count2 += one.len();
                    count4 += two.len();
                }
                (count2, count4)
            })
            .unwrap();

        std::thread::sleep(TEST_DURATION);

        // Stop and get results
        parser_sm.stop().unwrap();
        let (count2, count4) = thread_manager.join_thread::<(usize, usize)>(&dummy_session).unwrap();
        assert!(count2 > 0);
        assert!(count4 > 0);
        assert_eq!(count2, count4);
    }
}
