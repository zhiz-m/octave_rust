use std::{collections::VecDeque, sync::Arc};

use super::{
    config,
    song::{Song, SongPlayableState},
    subprocess::get_audio_reader_config,
};
use tokio::{
    sync::Mutex,
    task::JoinHandle,
    time::{sleep_until, Instant},
};

pub struct SongLoader {
    job_handle: JoinHandle<()>,
}

impl SongLoader {
    async fn loader_loop(songs: Arc<Mutex<VecDeque<Song>>>) {
        loop {
            sleep_until(
                Instant::now()
                    .checked_add(config::audio::SONG_LOADER_POLL_INTERVAL)
                    .unwrap(),
            )
            .await;
            let work = {
                let songs = songs.lock().await;
                songs.iter().find_map(|song| match &song.state {
                    SongPlayableState::Ready { .. } => None,
                    SongPlayableState::Waiting { work } => Some(work.clone()),
                })
            };
            if let Some(work) = work {
                match get_audio_reader_config(&work.query, work.stream_type).await {
                    anyhow::Result::Err(err) => {
                        log::error!("Error loading audio reader config {}", err)
                    }
                    anyhow::Result::Ok(config) => {
                        let mut songs = songs.lock().await;
                        songs.iter_mut().for_each(|song| match &song.state {
                            SongPlayableState::Ready { .. } => (),
                            SongPlayableState::Waiting { work: song_work } => {
                                if work.eq(song_work) {
                                    song.state = SongPlayableState::Ready {
                                        // todo: clone one more time than necessary
                                        config: config.clone(),
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    pub async fn cleanup(&mut self) -> anyhow::Result<()> {
        self.job_handle.abort();
        Ok(())
    }

    pub fn start_new(songs: Arc<Mutex<VecDeque<Song>>>) -> Self {
        let job_handle = tokio::spawn({
            let songs = songs.clone();
            async move { Self::loader_loop(songs).await }
        });
        Self { job_handle }
    }
}
