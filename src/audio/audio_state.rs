use std::{
    sync::Arc,
    mem::drop,
};
use rand::seq::SliceRandom;
use super::{
    song_queue::SongQueue,
    song::{
        Song,
    },
    song_searcher::{
        process_query,
        song_recommender,
    },
    subprocess::ffmpeg_pcm,
};
use crate::util::send_embed_http;
use songbird::{Call, Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent, 
    input::{
        self,
        reader::Reader,
    }, 
    tracks::{
        TrackHandle,
        TrackCommand,
    }

};
use tokio::sync::Mutex;
use serenity::{
    async_trait,
    prelude::{
        Mutex as SerenityMutex,
    },
    http::Http,
    client::Context,
    model::{
        id::ChannelId,
        channel::Message,
    },
};

pub struct AudioState{
    queue: SongQueue,
    handler: Arc<SerenityMutex<Call>>,
    current_song: Mutex<Option<Song>>,
    track_handle: Mutex<Option<TrackHandle>>,
    is_looping: Mutex<bool>,

    channel_id: Mutex<ChannelId>,
    http: Mutex<Arc<Http>>,
}

impl AudioState{
    pub fn new(handler: Arc<SerenityMutex<Call>>, ctx: &Context, msg: &Message) -> Arc<AudioState>{
        let audio_state = AudioState{
            queue: SongQueue::new(),
            handler,
            current_song: Mutex::new(None),
            track_handle: Mutex::new(None),
            is_looping: Mutex::new(false),

            channel_id: Mutex::new(msg.channel_id),
            http: Mutex::new(ctx.http.clone()),
        };
        let audio_state = Arc::new(audio_state);
        {
            let audio_state = audio_state.clone();
            tokio::spawn(async {
                AudioState::play_audio(audio_state).await;
            });
        }
        audio_state
    }

    pub async fn set_context(audio_state: Arc<AudioState>, ctx: &Context, msg: &Message){
        {
            let mut channel_id = audio_state.channel_id.lock().await;
            *channel_id = msg.channel_id;
        }
        {
            let mut http = audio_state.http.lock().await;
            *http = ctx.http.clone();
        }
    }

    async fn play_audio(audio_state: Arc<AudioState>){
        let is_looping = audio_state.is_looping.lock().await;
        let mut song = if *is_looping{
            let mut current_song = audio_state.current_song.lock().await;
            current_song.take().expect("logical error: expected current_song to be non-empty")
        }else{
            audio_state.queue.pop().await
        };
        drop(is_looping);

        
        let url = song.get_url().await;
        let source = ffmpeg_pcm(url).await;
        let source = match source {
            Ok(source) => source,
            Err(why) => {
                println!("Error in AudioState::play_audio: {}", why);
                return
            }
        };
        let reader = Reader::Extension(source);
        let source = input::Input::float_pcm(true, reader);

        let mut handler = audio_state.handler.lock().await;
        
        let handle = handler.play_source(source);

        if let Err(why) = handle.add_event(
            Event::Track(TrackEvent::End),
            SongEndNotifier{
                audio_state: audio_state.clone(),
            }
        ){
            panic!("Err AudioState::play_audio: {:?}", why);
        }
        {
            let text = song.get_string().await;
            let channel_id = audio_state.channel_id.lock().await;
            let http = audio_state.http.lock().await;
            send_embed_http(*channel_id, http.clone(), &format!(
                "Now playing:\n\n {}", text
            )).await;
        }
        let mut current_song = audio_state.current_song.lock().await;
        *current_song = Some(song);
        let mut track_handle = audio_state.track_handle.lock().await;
        *track_handle = Some(handle);
    }

    pub async fn add_audio(audio_state: Arc<AudioState>, query: &str, shuffle: bool){
        let mut songs = match process_query(query).await{
            Ok(songs) => songs,
            Err(why) => {
                println!("Error add_audio: {}", why);
                return;
            },
        };
        if shuffle {
            songs.shuffle(&mut rand::thread_rng());
        }
        audio_state.queue.push(songs).await;
    }

    pub async fn add_recommended_songs(audio_state: Arc<AudioState>, query: &str, amount: usize){
        let mut songs = match song_recommender(query, amount).await{
            Ok(songs) => songs,
            Err(why) => {
                println!("Error add_recommended_songs: {}", why);
                return;
            },
        };
        songs.shuffle(&mut rand::thread_rng());
        audio_state.queue.push(songs).await;
    }

    pub async fn extend_songs(audio_state: Arc<AudioState>, query: &str, extend_ratio: f64){
        let mut songs = match process_query(query).await{
            Ok(songs) => songs,
            Err(why) => {
                println!("Error extend_songs: {}", why);
                return;
            },
        };
        let recommended_songs = match song_recommender(query, (songs.len() as f64 * extend_ratio) as usize).await{
            Ok(songs) => songs,
            Err(why) => {
                println!("Error add_recommended_songs: {}", why);
                return;
            },
        };
        songs.extend(recommended_songs);
        songs.shuffle(&mut rand::thread_rng());
        audio_state.queue.push(songs).await;
    }

    pub async fn send_track_command(audio_state: Arc<AudioState>, cmd: TrackCommand) -> Result<(), String>{
        let track_handle = audio_state.track_handle.lock().await;
        match track_handle.as_ref() {
            Some(track_handle) => {
                match track_handle.send(cmd){
                    Ok(()) => Ok(()),
                    Err(why) => Err(format!("{:?}",why))
                }
            },
            None => Err("no song currently playing".to_string())
        }
    }

    pub async fn shuffle(audio_state: Arc<AudioState>) -> Result<(), String>{
        audio_state.queue.shuffle().await
    }

    pub async fn clear(audio_state: Arc<AudioState>) -> Result<(), String>{
        audio_state.queue.clear().await
    }

    // on success, returns a bool that specifies whether the queue is now being looped
    pub async fn change_looping(audio_state: Arc<AudioState>) -> Result<bool, String>{
        {
            let current_song = audio_state.current_song.lock().await;
            if current_song.is_none() {
                return Err("no song is playing".to_string());
            }
        }
        let mut is_looping = audio_state.is_looping.lock().await;
        *is_looping = !*is_looping;
        Ok(*is_looping)
        /*
        if looping{
            if *is_looping{
                Err("already looping".to_string())
            }else{
                *is_looping = true;
                Ok(())
            }
        }else{
            if !*is_looping{
                Err("not looping at the moment".to_string())
            }else{
                *is_looping = false;
                Ok(())
            }
        }*/
    }

    pub async fn cleanup(audio_state: Arc<AudioState>) {
        audio_state.queue.cleanup().await;
    }

    pub async fn get_string(audio_state: Arc<AudioState>) -> String {
        let current_song = audio_state.current_song.lock().await;
        let current_song = match &*current_song {
            Some(song) => song.get_string().await,
            None => "*Not playing*\n".to_string(),
        };
        format!("**Current Song:**\n{}\n\n**Queue:**\n{}", current_song, audio_state.queue.get_string().await)
    }
}

struct SongEndNotifier {
    audio_state: Arc<AudioState>,
}

#[async_trait]
impl VoiceEventHandler for SongEndNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        AudioState::play_audio(self.audio_state.clone()).await;

        None
    }
}