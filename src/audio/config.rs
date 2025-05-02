pub mod spotify_recommend {
    pub const SAME_ARTIST: u32 = 1;
    pub const EXPLORE_ARTIST: u32 = 1;
    pub const EXPLORE_ALBUM: u32 = 0;
}

pub mod audio {
    use std::time::Duration;

    //pub const EXTEND_RATIO: f64 = 1.5;
    // pub const TIMEOUT_DURATION: Duration = Duration::from_millis(600000);
    pub const AUDIO_LOOP_POLL_INTERVAL: Duration = Duration::from_millis(1000);
    pub const SONG_LOADER_POLL_INTERVAL: Duration = Duration::from_millis(1000);
    pub const YTDL_QUERY_RETRY_INTERVAL: Duration = Duration::from_millis(5000);
    pub const YTDL_DOWNLOAD_RETRY_INTERVAL: Duration = Duration::from_millis(10000);
    pub const GET_AUDIO_READER_NUM_RETRIES: usize = 3;
    //pub const AUDIO_NORM_DB: i32 = -10;
    pub const BOT_PREFIX: &str = "o.";
    pub const MESSAGE_UI_COMPONENT_CHAIN_INTERVAL_MS: u64 = 500;
}

pub mod env {
    pub const DISCORD_BOT_TOKEN: &str = "OCTAVE_BOT_TOKEN";
    pub const SPOTIFY_CLIENT_ID: &str = "SPOTIFY_CLIENT_ID";
    pub const SPOTIFY_CLIENT_SECRET: &str = "SPOTIFY_CLIENT_SECRET";
}
