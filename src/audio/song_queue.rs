use std::{
    sync::Arc,
    collections::VecDeque,
    mem::drop,
};
use tokio::sync::{Semaphore, Mutex};
use super::{
    song::Song,
    loader::Loader,
    work::Work,
};
pub struct SongQueue{
    loader: Loader,
    queue: Arc<Mutex<VecDeque<Song>>>,
    queue_sem: Semaphore,
}

impl SongQueue{
    pub fn new() -> SongQueue {
        SongQueue{
            loader: Loader::new(),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            queue_sem: Semaphore::new(0),
        }
    }
    pub async fn push(&self, songs: Vec<(Song, Work)>){
        let mut queue = self.queue.lock().await;
        let count = songs.len();
        for item in songs.into_iter(){
            queue.push_back(item.0);
            self.loader.add_work(item.1).await;
        }
        drop(queue);
        self.queue_sem.add_permits(count);
    }
    pub async fn pop(&self) -> Song {
        self.queue_sem.acquire().await.expect("Error SongQueue.pop: semaphore acquire failed").forget();
        let mut queue = self.queue.lock().await;
        queue.pop_front().expect("Error SongQueue.pop: semaphore sync failure")
    }
}