use crate::util::send_embed_http;
use rand::seq::SliceRandom;
use std::{
    mem::drop,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::config::audio as audio_config;
use super::{
    message_ui_component::MessageUiComponent,
    song::Song,
    song_queue::SongQueue,
    song_searcher::{process_query, song_recommender},
    subprocess::get_pcm_reader,
    work::StreamType,
};
use serenity::{
    async_trait,
    client::Context,
    model::{channel::Message, id::ChannelId},
    prelude::Mutex as SerenityMutex,
};
use songbird::{
    input::{self, reader::Reader},
    tracks::{TrackCommand, TrackHandle},
    Call, Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent,
};
use tokio::{
    sync::{Mutex, Semaphore},
    time::timeout,
};

pub struct AudioState {
    queue: SongQueue,
    handler: Arc<SerenityMutex<Call>>,
    current_song: Mutex<Option<Song>>,
    track_handle: Mutex<Option<TrackHandle>>,
    is_looping: Mutex<bool>,
    song_ready: Semaphore,
    current_stream_type: Mutex<StreamType>,
    is_paused: AtomicBool,

    channel_id: Mutex<ChannelId>,
    context: Mutex<Arc<Context>>,

    message_ui_component: Mutex<Option<MessageUiComponent>>,
}

impl AudioState {
    pub fn new(handler: Arc<SerenityMutex<Call>>, ctx: &Context, msg: &Message) -> Arc<AudioState> {
        let audio_state = AudioState {
            queue: SongQueue::new(),
            handler,
            current_song: Mutex::new(None),
            track_handle: Mutex::new(None),
            is_looping: Mutex::new(false),
            song_ready: Semaphore::new(1),
            current_stream_type: Mutex::new(StreamType::Online),
            is_paused: AtomicBool::new(false),

            channel_id: Mutex::new(msg.channel_id),
            context: Mutex::new(Arc::new(ctx.clone())),

            message_ui_component: Mutex::new(None),
        };
        let audio_state = Arc::new(audio_state);
        {
            let audio_state = audio_state.clone();
            tokio::spawn(async move {
                audio_state.play_audio_loop().await;
            });
        }
        audio_state
    }

    pub async fn set_context(self: &Arc<Self>, ctx: &Context, msg: &Message) {
        {
            let mut channel_id = self.channel_id.lock().await;
            *channel_id = msg.channel_id;
        }
        {
            let mut context = self.context.lock().await;
            *context = Arc::new(ctx.clone());
        }
    }

    async fn play_audio_loop(self: &Arc<Self>) {
        loop {
            match timeout(
                audio_config::TIMEOUT_DURATION,
                self.clone().song_ready.acquire(),
            )
            .await
            {
                Ok(x) => {
                    x.expect(
                        "Err AudioState::play_audio_loop: failed to acquire song_ready semaphore",
                    )
                    .forget();
                }
                _ => {
                    {
                        let mut handler = self.handler.lock().await;
                        if handler.leave().await.is_err() {
                            println!("AudioState::play_audio_loop: handler failed to leave");
                        };
                    }
                    self.cleanup().await;
                    return;
                }
            };

            let is_looping = self.is_looping.lock().await;
            let mut song = if *is_looping {
                let mut current_song = self.current_song.lock().await;
                current_song
                    .take()
                    .expect("logical error: expected current_song to be non-empty")
            } else {
                self.queue.pop().await
            };
            drop(is_looping);

            let buf_config = match song.get_buf_config().await {
                Some(buf) => buf,
                None => {
                    println!("AudioState::play_audio: no song buffer found");
                    self.clone().play_next_song();
                    continue;
                }
            };
            let source = get_pcm_reader(buf_config).await;
            let source = match source {
                Ok(source) => source,
                Err(why) => {
                    println!("Error in AudioState::play_audio: {}", why);
                    self.clone().play_next_song();
                    continue;
                }
            };
            let reader = Reader::Extension(source);
            let source = input::Input::float_pcm(true, reader);

            let mut handler = self.handler.lock().await;

            let handle = handler.play_source(source);

            if let Err(why) = handle.add_event(
                Event::Track(TrackEvent::End),
                SongEndNotifier {
                    audio_state: self.clone(),
                },
            ) {
                panic!("Err AudioState::play_audio: {:?}", why);
            }
            {
                let text = song.get_string().await;
                let channel_id = self.channel_id.lock().await;

                let context = self.context.lock().await;

                send_embed_http(
                    *channel_id,
                    context.http.clone(),
                    &format!("Now playing:\n\n {}", text),
                )
                .await;
            }

            self.display_ui().await;

            let mut current_song = self.current_song.lock().await;
            *current_song = Some(song);
            let mut track_handle = self.track_handle.lock().await;
            *track_handle = Some(handle);
        }
    }

    pub async fn display_ui(self: &Arc<Self>) {
        let channel_id = self.channel_id.lock().await;

        let context = self.context.lock().await;

        let mut ptr = self.message_ui_component.lock().await;
        let mut component = MessageUiComponent::new(self.clone(), *channel_id, context.clone());
        if let Err(why) = component.start().await {
            println!("Err AudioState::play_audio: {:?}", why);
        }
        *ptr = Some(component);
    }

    pub fn play_next_song(self: &Arc<Self>) {
        self.song_ready.add_permits(1);
    }

    pub async fn add_audio(self: &Arc<Self>, query: &str, shuffle: bool) {
        let mut songs = match process_query(query, *self.current_stream_type.lock().await).await {
            Ok(songs) => songs,
            Err(why) => {
                println!("Error add_audio: {}", why);
                return;
            }
        };
        if shuffle {
            songs.shuffle(&mut rand::thread_rng());
        }
        self.queue.push(songs).await;
    }

    pub async fn add_recommended_songs(self: &Arc<Self>, query: &str, amount: usize) {
        let songs =
            match song_recommender(query, amount, *self.current_stream_type.lock().await).await {
                Ok(songs) => songs,
                Err(why) => {
                    println!("Error add_recommended_songs: {}", why);
                    return;
                }
            };
        self.queue.push(songs).await;
    }

    pub async fn extend_songs(self: &Arc<Self>, query: &str, extend_ratio: f64) {
        let mut songs = match process_query(query, *self.current_stream_type.lock().await).await {
            Ok(songs) => songs,
            Err(why) => {
                println!("Error extend_songs: {}", why);
                return;
            }
        };
        let recommended_songs = match song_recommender(
            query,
            (songs.len() as f64 * extend_ratio) as usize,
            *self.current_stream_type.lock().await,
        )
        .await
        {
            Ok(songs) => songs,
            Err(why) => {
                println!("Error add_recommended_songs: {}", why);
                return;
            }
        };
        songs.extend(recommended_songs);
        songs.shuffle(&mut rand::thread_rng());
        self.queue.push(songs).await;
    }

    pub async fn send_track_command(self: &Arc<Self>, cmd: TrackCommand) -> Result<(), String> {
        let track_handle = self.track_handle.lock().await;
        match track_handle.as_ref() {
            Some(track_handle) => match track_handle.send(cmd) {
                Ok(()) => Ok(()),
                Err(why) => Err(format!("{:?}", why)),
            },
            None => Err("no song currently playing".to_string()),
        }
    }

    pub async fn pause_resume(self: &Arc<Self>) -> Result<(), String> {
        let prev = self.is_paused.fetch_xor(true, Ordering::Relaxed);
        // not paused previously
        if prev {
            self.send_track_command(TrackCommand::Play).await
        } else {
            self.send_track_command(TrackCommand::Pause).await
        }
    }

    pub async fn shuffle(self: &Arc<Self>) -> Result<(), String> {
        self.queue.shuffle().await
    }

    pub async fn clear(self: &Arc<Self>) -> Result<(), String> {
        self.queue.clear().await
    }

    // on success, returns a bool that specifies whether the queue is now being looped
    pub async fn change_looping(self: &Arc<Self>) -> Result<bool, String> {
        {
            let current_song = self.current_song.lock().await;
            if current_song.is_none() {
                return Err("no song is playing".to_string());
            }
        }
        let mut is_looping = self.is_looping.lock().await;
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

    pub async fn change_stream_type(self: &Arc<Self>, stream_type: &str) -> Result<bool, String> {
        match stream_type.trim().to_lowercase().as_str() {
            "online" => {
                *self.current_stream_type.lock().await = StreamType::Online;
                Ok(true)
            }
            "loudnorm" => {
                *self.current_stream_type.lock().await = StreamType::Loudnorm;
                Ok(true)
            }
            _ => Err("Invalid input, accepted args are 'online' and 'loudnorm'".to_string()),
        }
    }

    pub async fn cleanup(self: &Arc<Self>) {
        self.queue.cleanup().await;
    }

    pub async fn get_string(self: &Arc<Self>) -> String {
        let current_song = self.current_song.lock().await;
        let current_song = match &*current_song {
            Some(song) => song.get_string().await,
            None => "*Not playing*\n".to_string(),
        };
        format!(
            "**Current Song:**\n{}\n\n**Queue:**\n{}",
            current_song,
            self.queue.get_string().await
        )
    }
}

struct SongEndNotifier {
    audio_state: Arc<AudioState>,
}

#[async_trait]
impl VoiceEventHandler for SongEndNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        self.audio_state.play_next_song();

        let mut current_song = self.audio_state.current_song.lock().await;
        *current_song = None;
        let mut track_handle = self.audio_state.track_handle.lock().await;
        *track_handle = None;

        None
    }
}
