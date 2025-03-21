use super::subprocess::PcmReaderConfig;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[derive(Copy, Clone)]
pub enum QueuePosition {
    Front,
    Back,
}

impl Default for QueuePosition {
    fn default() -> Self {
        Self::Front
    }
}

#[derive(Copy, Clone)]
pub enum StreamType {
    Online,
    Loudnorm,
}

impl Default for StreamType {
    fn default() -> Self {
        Self::Loudnorm
    }
}

#[derive(Clone)]
pub struct Work {
    pub sender: mpsc::Sender<Option<PcmReaderConfig>>,
    pub query: String,
    pub is_loaded: Arc<Mutex<bool>>,
    pub stream_type: StreamType,
}
