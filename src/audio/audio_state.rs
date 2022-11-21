use crate::{util::send_embed_http, PoiseContext};
use anyhow::anyhow;
use async_trait::async_trait;
use rand::seq::SliceRandom;
use std::{
    mem::drop,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
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
use songbird::{
    input::{self, reader::Reader},
    tracks::{TrackCommand, TrackHandle},
    Call, Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent,
};
use tokio::{
    sync::{Mutex, Semaphore},
    time::timeout,
};
use poise::serenity_prelude::{Mutex as SerenityMutex, ChannelId, Context};

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
    pub fn new(handler: Arc<SerenityMutex<Call>>, ctx: &PoiseContext<'_>) -> Arc<AudioState> {
        let audio_state = AudioState {
            queue: SongQueue::new(),
            handler,
            current_song: Mutex::new(None),
            track_handle: Mutex::new(None),
            is_looping: Mutex::new(false),
            song_ready: Semaphore::new(1),
            current_stream_type: Mutex::new(StreamType::Online),
            is_paused: AtomicBool::new(false),

            channel_id: Mutex::new(ctx.channel_id()),
            context: Mutex::new(Arc::new(ctx.discord().clone())),

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

    pub async fn set_context<'a>(self: &Arc<Self>, ctx: &PoiseContext<'a>) {
        {
            let mut channel_id = self.channel_id.lock().await;
            *channel_id = ctx.channel_id();
        }
        {
            let mut context = self.context.lock().await;
            *context = Arc::new(ctx.discord().clone());
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
                            log::error!("AudioState::play_audio_loop: handler failed to leave");
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
                    log::error!("AudioState::play_audio: no song buffer found");
                    self.play_next_song();
                    continue;
                }
            };
            let source = get_pcm_reader(buf_config).await;
            let source = match source {
                Ok(source) => source,
                Err(why) => {
                    log::error!("Error in AudioState::play_audio: {}", why);
                    self.play_next_song();
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
                log::error!("Err AudioState::play_audio: {:?}", why);
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
            log::error!("Err AudioState::play_audio: {:?}", why);
        }
        *ptr = Some(component);
    }

    pub fn play_next_song(self: &Arc<Self>) {
        self.song_ready.add_permits(1);
    }

    pub async fn add_audio(self: &Arc<Self>, query: &str, shuffle: bool) -> anyhow::Result<()> {
        let mut songs = process_query(query, *self.current_stream_type.lock().await).await?;
        if shuffle {
            songs.shuffle(&mut rand::thread_rng());
        }
        self.queue.push(songs).await;
        Ok(())
    }

    pub async fn add_recommended_songs(self: &Arc<Self>, query: &str, amount: usize) -> anyhow::Result<()> {
        let songs =
            song_recommender(query, amount, *self.current_stream_type.lock().await).await?;
        self.queue.push(songs).await;
        Ok(())
    }

    pub async fn extend_songs(self: &Arc<Self>, query: &str, extend_ratio: f64) -> anyhow::Result<()> {
        let mut songs = process_query(query, *self.current_stream_type.lock().await).await?;
        let recommended_songs = song_recommender(
            query,
            (songs.len() as f64 * extend_ratio) as usize,
            *self.current_stream_type.lock().await,
        )
        .await?;
        songs.extend(recommended_songs);
        songs.shuffle(&mut rand::thread_rng());
        self.queue.push(songs).await;
        Ok(())
    }

    pub async fn send_track_command(self: &Arc<Self>, cmd: TrackCommand) -> anyhow::Result<()> {
        let track_handle = self.track_handle.lock().await;
        match track_handle.as_ref() {
            Some(track_handle) => track_handle.send(cmd).map_err(|e| anyhow!(e.to_string())),
            None => Err(anyhow!("no song currently playing")),
        }
    }

    pub async fn pause_resume(self: &Arc<Self>, try_pause: Option<bool>) -> anyhow::Result<()> {
        // if None, then we reverse the current play/pause state
        let try_pause = try_pause.unwrap_or(!self.is_paused.fetch_xor(true, Ordering::Relaxed));
        // not paused previously
        if try_pause {
            self.send_track_command(TrackCommand::Pause).await
        } else {
            self.send_track_command(TrackCommand::Play).await
        }
    }

    pub async fn shuffle(self: &Arc<Self>) -> anyhow::Result<()> {
        self.queue.shuffle().await
    }

    pub async fn clear(self: &Arc<Self>) -> anyhow::Result<()> {
        self.queue.clear().await
    }

    // on success, returns a bool that specifies whether the queue is now being looped
    pub async fn change_looping(self: &Arc<Self>, try_loop: Option<bool>) -> anyhow::Result<bool> {
        {
            let current_song = self.current_song.lock().await;
            if current_song.is_none() {
                return Err(anyhow!("no song is playing"));
            }
        }
        let mut is_looping = self.is_looping.lock().await;
        *is_looping = try_loop.unwrap_or(!*is_looping);
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

    pub async fn change_stream_type(self: &Arc<Self>, stream_type: &str) -> anyhow::Result<()> {
        match stream_type.trim().to_lowercase().as_str() {
            "online" => {
                *self.current_stream_type.lock().await = StreamType::Online;
                Ok(())
            }
            "loudnorm" => {
                *self.current_stream_type.lock().await = StreamType::Loudnorm;
                Ok(())
            }
            _ => Err(anyhow!("Invalid input, accepted args are 'online' and 'loudnorm'")),
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
        log::info!("song ended, {:?}", SystemTime::now());
        self.audio_state.play_next_song();

        let mut current_song = self.audio_state.current_song.lock().await;
        *current_song = None;
        let mut track_handle = self.audio_state.track_handle.lock().await;
        *track_handle = None;

        None
    }
}
