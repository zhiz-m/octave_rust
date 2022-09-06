pub mod spotify_recommend {
    pub const SAME_ARTIST: u32 = 1;
    pub const EXPLORE_ARTIST: u32 = 1;
    pub const EXPLORE_ALBUM: u32 = 0;
}

pub mod audio {
    use std::time::Duration;

    pub const EXTEND_RATIO: f64 = 1.5;
    pub const TIMEOUT_DURATION: Duration = Duration::from_millis(600000);
    pub const AUDIO_NORM_DB: i32 = -10;
    pub const BOT_PREFIX: &str = "o."; 
    pub const MESSAGE_UI_COMPONENT_PROCESING_INTERVAL_MS: u64 = 500;
}
