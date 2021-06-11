use std::{
    sync::Arc,
    collections::VecDeque,
    mem::drop,
};
use tokio::sync::{Semaphore, Mutex};
use rand::seq::SliceRandom;
use super::{
    song::{
        Song,
        SongUrlState,
    },
    loader::Loader,
    work::Work,
};
pub struct SongQueue{
    loader: Arc<Mutex<Loader>>,
    queue: Arc<Mutex<VecDeque<Song>>>,
    queue_sem: Semaphore,
}

impl SongQueue{
    pub fn new() -> SongQueue {
        SongQueue{
            loader: Arc::new(Mutex::new(Loader::new())),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            queue_sem: Semaphore::new(0),
        }
    }
    pub async fn push(&self, songs: Vec<(Song, Option<Work>)>){
        let mut queue = self.queue.lock().await;
        let count = songs.len();
        let loader = self.loader.lock().await;
        for item in songs.into_iter(){
            queue.push_back(item.0);
            if let Some(work) = item.1{
                loader.add_work(work).await;
            };
            //self.loader.add_work(item.1).await;
        }
        drop(queue);
        self.queue_sem.add_permits(count);
    }
    pub async fn pop(&self) -> Song {
        self.queue_sem.acquire().await.expect("Error SongQueue.pop: semaphore acquire() failed").forget();
        let mut queue = self.queue.lock().await;
        queue.pop_front().expect("Error SongQueue.pop: semaphore sync failure")
    }
    pub async fn shuffle(&self) -> Result<(), String> {
        let mut queue = self.queue.lock().await;
        if queue.len() == 0 {
            return Err("queue is empty".to_string());
        }
        queue.make_contiguous().shuffle(&mut rand::thread_rng());

        self.reset_loader().await;
        let loader = self.loader.lock().await;

        for song in queue.iter(){
            match &song.url_state{
                SongUrlState::Proc{work,..} => loader.add_work(work.clone()).await,
            };
        };

        Ok(())
    }
    pub async fn clear(&self) -> Result<(), String>{
        let mut queue = self.queue.lock().await;
        if queue.len() == 0 {
            return Err("queue is empty".to_string());
        };
        queue.clear();
        Ok(())
    }
    async fn reset_loader(&self) {
        let mut loader = self.loader.lock().await;
        loader.cleanup().await;
        *loader = Loader::new();
    }
    pub async fn cleanup(&self) {
        let mut loader = self.loader.lock().await;
        loader.cleanup().await;
    }
    pub async fn get_string(&self) -> String{
        let queue = self.queue.lock().await;
        if queue.len() == 0 {
            return "*empty*".to_string();
        };
        let mut s = String::new();
        for song in queue.iter(){
            s += &song.get_string().await;
            s += "\n";
        }
        s
    }
}