use std::{
    sync::{Arc},
};
use tokio::sync::{mpsc, Mutex};
use super::{
    work::Work,
};

pub enum SongUrlState{
    Proc{
        is_loaded: Arc<Mutex<bool>>,
        receiver: mpsc::Receiver<String>,
        work: Work,
    }
}

pub struct SongMetadata{
    pub artist: Option<String>,
    pub title: Option<String>,
    pub search_query: Option<String>,
    pub youtube_url: Option<String>,
    pub duration: Option<u64>,
}

pub struct Song{
    pub url_state: SongUrlState,
    url: Arc<Mutex<Option<String>>>,
    metadata: SongMetadata,
}

impl Song{
    pub fn new_load(metadata: SongMetadata) -> Option<(Song, Option<Work>)>{
        let query = match metadata.youtube_url.clone(){
            Some(url) => url,
            None => format!("ytsearch:{}", metadata.search_query.clone()?),
        };

        let (tx, rx) = mpsc::channel(1);
        let is_loaded = Arc::new(Mutex::new(false));
        let work = Work{
            sender: tx,
            is_loaded: is_loaded.clone(),
            query: query.clone(),
        };
        let url_state = SongUrlState::Proc{
            is_loaded: is_loaded.clone(),
            receiver: rx,
            work: work.clone(),
        };
        
        let song = Song{
            url_state,
            url: Arc::new(Mutex::new(None)),
            metadata,
        };
        Some((song, Some(work)))
    }
    pub fn get_metadata(&self) -> &SongMetadata {
        return &self.metadata;
    }
    pub async fn get_url(&mut self) -> String{
        match &mut self.url_state{
            SongUrlState::Proc{receiver,..} => {
                let mut url = self.url.lock().await;
                match url.clone(){
                    Some(url)=>{
                        return url;
                    }
                    None=>{
                        //drop(url);
                        let source = receiver.recv().await.unwrap();
                        //let mut url = song.url.lock().unwrap();
                        *url = Some(source);
                        return url.clone().unwrap();
                    }
                }
            },
        };
    }
    pub async fn get_string(&self) -> String{
        let metadata = &self.metadata;
        let artist = match &metadata.artist{
            Some(artist) => artist,
            None => "unknown",
        };
        let title = match &metadata.artist{
            Some(title) => title,
            None => "unknown",
        };
        let duration = match &metadata.duration{
            Some(duration) => {
                let mins = duration / 60;
                let secs = duration - mins * 60;
                format!("{}:{:0>2}", mins, secs)
            },
            None => "unknown duration".to_string(),
        };
        format!("{} by {} | {}", title, artist, &duration)
    }
}

/*
async fn send_url(state: &SongUrlState, url: String) {
    let SongUrlState::Proc{is_loaded, sender, ..} = state;
    let is_loaded = is_loaded.lock().unwrap();
    if *is_loaded{
        return
    }
    sender.send(url).await.unwrap();
}*/
/*
async fn hi() -> Song{
    let (tx, rx) = mpsc::channel::<String>(100);
    Song{
        url: "".to_string(),
        url_state: SongUrlState::Proc{
            is_loaded: Arc::new(Mutex::new(false)),
            is_waited: Arc::new(Mutex::new(false)),
            receiver: rx,
            //sender: tx,
        }
    }
}*/

/*
async fn get_url(song: &mut Song) -> &str{
    let is_waited = song.is_loaded.lock().unwrap();
    if *is_waited{
        return &song.url;
    }
    let wait_chan = &mut song.wait_chan;
    song.url = wait_chan.await.unwrap();
    &song.url
}*/