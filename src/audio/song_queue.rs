use super::{
    song::{Song, SongBufConfigState},
    work::Work,
    youtube_loader::YoutubeLoader,
};
use anyhow::anyhow;
use rand::seq::SliceRandom;
use std::{cmp::min, collections::VecDeque, mem::drop, sync::Arc};
use tokio::sync::{Mutex, Semaphore};
pub struct SongQueue {
    loader: Arc<Mutex<YoutubeLoader>>,
    queue: Arc<Mutex<VecDeque<Song>>>,
    queue_sem: Semaphore,
}

impl SongQueue {
    pub fn new() -> SongQueue {
        SongQueue {
            loader: Arc::new(Mutex::new(YoutubeLoader::new())),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            queue_sem: Semaphore::new(0),
        }
    }
    pub async fn push(&self, songs: Vec<(Song, Option<Work>)>) -> anyhow::Result<()> {
        let mut queue = self.queue.lock().await;
        let count = songs.len();
        let loader = self.loader.lock().await;
        for item in songs.into_iter() {
            queue.push_back(item.0);
            if let Some(work) = item.1 {
                loader.add_work(work).await?;
            };
            //self.loader.add_work(item.1).await;
        }
        drop(queue);
        self.queue_sem.add_permits(count);
        Ok(())
    }
    pub async fn pop(&self) -> Song {
        self.queue_sem
            .acquire()
            .await
            .expect("Error SongQueue.pop: semaphore acquire() failed")
            .forget();
        let mut queue = self.queue.lock().await;
        queue
            .pop_front()
            .expect("Error SongQueue.pop: semaphore sync failure")
    }
    pub async fn shuffle(&self) -> anyhow::Result<()> {
        let mut queue = self.queue.lock().await;
        if queue.len() == 0 {
            return Err(anyhow!("queue is empty"));
        }
        queue.make_contiguous().shuffle(&mut rand::thread_rng());

        self.reset_loader().await?;
        let loader = self.loader.lock().await;

        for song in queue.iter() {
            match &song.buf_config_state {
                SongBufConfigState::Proc { work, .. } => loader.add_work(work.clone()).await,
            }?;
        }

        Ok(())
    }
    pub async fn clear(&self) -> anyhow::Result<()> {
        while self.queue_sem.available_permits() > 0 {
            self.pop().await;
        }
        self.reset_loader().await?;
        /*let mut queue = self.queue.lock().await;
        if queue.len() == 0 {
            return Err("queue is empty".to_string());
        };
        queue.clear();*/
        Ok(())
    }
    async fn reset_loader(&self) -> anyhow::Result<()> {
        let mut loader = self.loader.lock().await;
        loader.cleanup().await?;
        *loader = YoutubeLoader::new();
        Ok(())
    }
    pub async fn cleanup(&self) -> anyhow::Result<()> {
        let mut loader = self.loader.lock().await;
        loader.cleanup().await?;
        Ok(())
    }
    pub async fn get_string(&self) -> String {
        let queue = self.queue.lock().await;
        if queue.len() == 0 {
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
