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

#[derive(Copy, Clone, PartialEq, Eq)]
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
pub enum AudioReaderConfig {
    Online { src_url: String },
    Loudnorm { buf: Vec<u8> },
}

#[derive(Clone, PartialEq, Eq)]
pub struct SongLoaderWork {
    pub query: String,
    pub stream_type: StreamType,
}
