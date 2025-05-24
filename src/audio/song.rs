use super::types::{AudioReaderConfig, SongLoaderWork, StreamType};

pub enum SongPlayableState {
    Waiting { work: SongLoaderWork },
    Ready { config: AudioReaderConfig },
}

#[derive(Clone)]
pub enum HowToFind {
    SearchQuery(String),
    YoutubeTrackUrl(String),
}

pub struct SongMetadata {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub how_to_find: HowToFind,
    pub duration: Option<u64>,
}

pub struct Song {
    pub state: SongPlayableState,
    metadata: SongMetadata,
}

impl Song {
    pub fn new_load(metadata: SongMetadata, stream_type: StreamType) -> Self {
        let query = match metadata.how_to_find.clone() {
            HowToFind::YoutubeTrackUrl(url) => url,
            HowToFind::SearchQuery(query) => format!("ytsearch:{} official music", query),
        };

        let work = SongLoaderWork { query, stream_type };
        let state = SongPlayableState::Waiting { work };
        Song { state, metadata }
    }

    pub fn get_buf_config(&self) -> Option<AudioReaderConfig> {
        match &self.state {
            // SongPlayableState::Proc { receiver, .. } => match &self.buf_config {
            //     Some(buf_config) => Some(buf_config.clone()),
            //     None => {
            //         let source = receiver.recv().await.unwrap();
            //         self.buf_config = source;
            //         self.buf_config.clone()
            //     }
            // },
            // todo: potentially unnecessary clone
            SongPlayableState::Ready { config } => Some(config.clone()),
            SongPlayableState::Waiting { .. } => None,
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
