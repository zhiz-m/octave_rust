use super::{
    subprocess::PcmReaderConfig,
    types::{StreamType, Work},
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub enum SongBufConfigState {
    Proc {
        receiver: mpsc::Receiver<Option<PcmReaderConfig>>,
        work: Work,
    },
}

pub struct SongMetadata {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub search_query: Option<String>,
    pub youtube_url: Option<String>,
    pub duration: Option<u64>,
}

pub struct Song {
    pub buf_config_state: SongBufConfigState,
    buf_config: Arc<Mutex<Option<PcmReaderConfig>>>,
    metadata: SongMetadata,
}

impl Song {
    pub fn new_load(
        metadata: SongMetadata,
        stream_type: StreamType,
    ) -> Option<(Song, Option<Work>)> {
        let query = match metadata.youtube_url.clone() {
            Some(url) => url,
            None => format!("ytsearch:{} official music", metadata.search_query.clone()?),
        };

        let (tx, rx) = mpsc::channel(1);
        let work = Work {
            sender: tx,
            is_loaded: Arc::new(Mutex::new(false)),
            query,
            stream_type,
        };
        let buf_config_state = SongBufConfigState::Proc {
            receiver: rx,
            work: work.clone(),
        };

        let song = Song {
            buf_config_state,
            buf_config: Arc::new(Mutex::new(None)),
            metadata,
        };
        Some((song, Some(work)))
    }
    pub async fn get_buf_config(&mut self) -> Option<PcmReaderConfig> {
        match &mut self.buf_config_state {
            SongBufConfigState::Proc { receiver, .. } => {
                let mut buf_config = self.buf_config.lock().await;
                match buf_config.clone() {
                    Some(buf_config) => Some(buf_config),
                    None => {
                        let source = receiver.recv().await.unwrap();
                        *buf_config = source;
                        buf_config.clone()
                    }
                }
            }
        }
    }
    pub async fn get_string(&self) -> String {
        let metadata = &self.metadata;
        let artist = match &metadata.artist {
            Some(artist) => artist,
            None => "unknown",
        };
        let title = match &metadata.title {
            Some(title) => title,
            None => "unknown",
        };
        let duration = match &metadata.duration {
            Some(duration) => {
                let mins = duration / 60;
                let secs = duration - mins * 60;
                format!("{}:{:0>2}", mins, secs)
            }
            None => "unknown duration".to_string(),
        };
        format!("{} by {} | {}", title, artist, &duration)
    }
}
