use super::{song::Song, song_loader::SongLoader, types::QueuePosition};
use anyhow::anyhow;
use rand::seq::SliceRandom;
use std::{cmp::min, collections::VecDeque, sync::Arc};
use tokio::sync::Mutex;
pub struct SongQueue {
    loader: Arc<Mutex<SongLoader>>,
    queue: Arc<Mutex<VecDeque<Song>>>,
}

impl SongQueue {
    pub fn new() -> SongQueue {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let loader = Arc::new(Mutex::new(SongLoader::start_new(queue.clone())));
        SongQueue { loader, queue }
    }
    pub async fn push(
        &self,
        mut songs: Vec<Song>,
        queue_position: QueuePosition,
    ) -> anyhow::Result<()> {
        let mut queue = self.queue.lock().await;
        if let QueuePosition::Front = queue_position {
            songs.reverse();
        };
        let push = match queue_position {
            QueuePosition::Back => VecDeque::push_back,
            QueuePosition::Front => VecDeque::push_front,
        };
        for song in songs.into_iter() {
            push(&mut queue, song);
        }
        Ok(())
    }
    pub async fn try_pop_ready_song(&self) -> Option<Song> {
        let mut queue = self.queue.lock().await;
        let next_song = queue.front();
        let audio_reader_config = next_song.and_then(Song::get_buf_config);
        if let (Some(_), Some(_)) = (next_song, audio_reader_config) {
            queue.pop_front()
        } else {
            None
        }
    }
    pub async fn shuffle(&self) -> anyhow::Result<()> {
        let mut queue = self.queue.lock().await;
        if queue.is_empty() {
            return Err(anyhow!("queue is empty"));
        }
        queue.make_contiguous().shuffle(&mut rand::thread_rng());

        Ok(())
    }
    pub async fn clear(&self) -> anyhow::Result<()> {
        let mut queue = self.queue.lock().await;
        queue.clear();
        Ok(())
    }
    pub async fn cleanup(&self) -> anyhow::Result<()> {
        let mut loader = self.loader.lock().await;
        loader.cleanup().await?;
        Ok(())
    }
    pub async fn get_string(&self) -> String {
        let queue = self.queue.lock().await;
        if queue.is_empty() {
            return "*empty*".to_string();
        };
        let mut s = String::new();
        s += &format!(
            "*Showing {} of {} songs*\n",
            min(20, queue.len()),
            queue.len()
        );
        for (i, song) in queue.iter().take(20).enumerate() {
            s += &format!("{}. ", i + 1);
            s += &song.get_string().await;
            s += "\n";
        }
        s
    }
}
