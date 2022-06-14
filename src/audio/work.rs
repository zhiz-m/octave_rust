use std::{
    sync::{Arc},
};
use tokio::sync::{mpsc, Mutex};
use super::subprocess::PcmReaderConfig;

#[derive(Copy, Clone)]
pub enum StreamType{
    Online,
    Loudnorm,
}
#[derive(Clone)]
pub struct Work{
    pub sender: mpsc::Sender<Option<PcmReaderConfig>>,
    pub query: String,
    pub is_loaded: Arc<Mutex<bool>>,
    pub stream_type: StreamType
}