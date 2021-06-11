use std::{
    sync::Arc,
};
use rand::seq::SliceRandom;
use super::{
    song_queue::SongQueue,
    song::{
        Song,
    },
    query::process_query,
    subprocess::ffmpeg_pcm,
};
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
    }
};

pub struct AudioState{
    queue: SongQueue,
    handler: Arc<SerenityMutex<Call>>,
    current_song: Mutex<Option<Song>>,
    track_handle: Mutex<Option<TrackHandle>>,
}

impl AudioState{
    pub fn new(handler: Arc<SerenityMutex<Call>>) -> Arc<AudioState>{
        let audio_state = AudioState{
            queue: SongQueue::new(),
            handler: handler.clone(),
            current_song: Mutex::new(None),
            track_handle: Mutex::new(None),
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

    async fn play_audio(audio_state: Arc<AudioState>){
        let mut song = audio_state.queue.pop().await;
        println!("hi3");
        let url = song.get_url().await;
        println!("hi4 {}", url);
        let source = ffmpeg_pcm(url).await;
        let source = match source {
            Ok(source) => source,
            Err(why) => {
                println!("Error in AudioState::play_audio: {}",why);
                return
            }
        };
        let reader = Reader::Extension(source);
        let source = input::Input::float_pcm(true, reader);

        println!("hi4.5");
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
        
        let mut current_song = audio_state.current_song.lock().await;
        *current_song = Some(song);
        let mut track_handle = audio_state.track_handle.lock().await;
        *track_handle = Some(handle);
        println!("hi5");
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

    pub async fn send_track_command(audio_state: Arc<AudioState>, cmd: TrackCommand) -> Result<(), String>{
        let track_handle = audio_state.track_handle.lock().await;
        match &*track_handle {
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

    pub async fn cleanup(audio_state: Arc<AudioState>) {
        audio_state.queue.cleanup().await;
    }

    pub async fn get_string(audio_state: Arc<AudioState>) -> String {
        let current_song = audio_state.current_song.lock().await;
        let current_song = match &*current_song {
            Some(song) => song.get_string().await,
            None => "*Not playing*\n".to_string(),
        };
        format!("**Current Song:**\n{}\n**Queue:**\n{}", current_song, audio_state.queue.get_string().await)
    }
}

struct SongEndNotifier {
    //chan_id: ChannelId,
    //http: Arc<Http>,
    audio_state: Arc<AudioState>,
}

#[async_trait]
impl VoiceEventHandler for SongEndNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        println!("SongEndNotifier event has started");
        AudioState::play_audio(self.audio_state.clone()).await;

        None
    }
}