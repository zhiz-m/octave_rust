use crate::{util::send_embed, PoiseContext};
use anyhow::anyhow;
use async_trait::async_trait;
use rand::seq::SliceRandom;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
};

use super::{config, types::QueuePosition};
use super::{
    message_ui_component::MessageUiComponent,
    song::Song,
    song_queue::SongQueue,
    song_searcher::{process_query, song_recommender},
    subprocess::get_audio_reader,
    types::StreamType,
};
use poise::serenity_prelude::{ChannelId, Context};
use songbird::{
    error::TrackResult,
    input::{
        self,
        core::io::{MediaSourceStream, MediaSourceStreamOptions},
        AudioStream, LiveInput,
    },
    tracks::TrackHandle,
    Call, Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent,
};
use tokio::{
    sync::Mutex,
    time::{sleep_until, Instant},
};

pub struct AudioState {
    queue: SongQueue,
    handler: Arc<Mutex<Call>>,
    current_song: Mutex<Option<Song>>,
    next_looping_song_to_play: Mutex<Option<Song>>,
    track_handle: Mutex<Option<TrackHandle>>,
    is_looping: Mutex<bool>,
    // song_ready: Semaphore,
    current_stream_type: Mutex<StreamType>,
    is_paused: AtomicBool,

    channel_id: Mutex<ChannelId>,
    context: Mutex<Arc<Context>>,

    message_ui_component: Mutex<Option<MessageUiComponent>>,
}

impl AudioState {
    pub fn new(handler: Arc<Mutex<Call>>, ctx: &PoiseContext<'_>) -> Arc<AudioState> {
        let audio_state = AudioState {
            queue: SongQueue::new(),
            handler,
            current_song: Mutex::new(None),
            next_looping_song_to_play: Mutex::new(None),
            track_handle: Mutex::new(None),
            is_looping: Mutex::new(false),
            // song_ready: Semaphore::new(1),
            current_stream_type: Mutex::new(StreamType::Loudnorm),
            is_paused: AtomicBool::new(false),

            channel_id: Mutex::new(ctx.channel_id()),
            context: Mutex::new(Arc::new(ctx.serenity_context().clone())),

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

    pub async fn set_context(&self, ctx: &PoiseContext<'_>) {
        {
            let mut channel_id = self.channel_id.lock().await;
            *channel_id = ctx.channel_id();
        }
        {
            let mut context = self.context.lock().await;
            *context = Arc::new(ctx.serenity_context().clone());
        }
    }

    async fn play_audio_loop(self: &Arc<Self>) {
        loop {
            sleep_until(
                Instant::now()
                    .checked_add(config::audio::AUDIO_LOOP_POLL_INTERVAL)
                    .unwrap(),
            )
            .await;

            {
                if self.current_song.lock().await.is_some() {
                    continue;
                }
            }

            let next_looping_song_to_play = { self.next_looping_song_to_play.lock().await.take() };
            let next_song = match next_looping_song_to_play {
                Some(song) => Some(song),
                None => self.queue.try_pop_ready_song().await,
            };
            let audio_reader_config = next_song.as_ref().and_then(Song::get_buf_config);
            if let (Some(song), Some(buf_config)) = (next_song, audio_reader_config) {
                let source = get_audio_reader(buf_config).await;
                let source = match source {
                    Ok(source) => source,
                    Err(why) => {
                        log::error!("Error in AudioState::play_audio: {}", why);
                        // self.play_next_song();
                        continue;
                    }
                };
                let input = input::Input::Live(
                    LiveInput::Wrapped(AudioStream {
                        input: MediaSourceStream::new(source, MediaSourceStreamOptions::default()),
                        hint: None,
                    }),
                    None,
                );

                let mut handler = self.handler.lock().await;

                let handle = handler.play_input(input);

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

                    if let Err(why) = send_embed(
                        &context.http,
                        *channel_id,
                        &format!("Now playing:\n\n {}", text),
                    )
                    .await
                    {
                        log::error!("Err AudioState::play_audio: {:?}", why);
                    }
                }

                if let Err(why) = self.display_ui().await {
                    log::error!("Err AudioState::play_audio: {:?}", why);
                }

                let mut current_song = self.current_song.lock().await;
                *current_song = Some(song);
                let mut track_handle = self.track_handle.lock().await;
                *track_handle = Some(handle);
            }
        }
    }

    pub async fn display_ui(self: &Arc<Self>) -> anyhow::Result<()> {
        let channel_id = self.channel_id.lock().await;

        let context = self.context.lock().await;

        let mut ptr = self.message_ui_component.lock().await;
        let mut component = MessageUiComponent::new(self.clone(), context.clone());
        component.start_with_channel_id(*channel_id).await?;
        *ptr = Some(component);
        Ok(())
    }

    pub async fn display_ui_with_poise_context_reply(
        self: &Arc<Self>,
        ctx: &PoiseContext<'_>,
    ) -> anyhow::Result<()> {
        let context = self.context.lock().await;

        let mut ptr = self.message_ui_component.lock().await;
        let mut component = MessageUiComponent::new(self.clone(), context.clone());
        component.start_with_poise_context(ctx).await?;
        *ptr = Some(component);
        Ok(())
    }

    pub async fn add_audio(
        &self,
        query: &str,
        queue_position: QueuePosition,
        shuffle: bool,
        stream_type: StreamType,
    ) -> anyhow::Result<()> {
        let mut songs = process_query(query, stream_type).await?;
        if shuffle {
            songs.shuffle(&mut rand::thread_rng());
        }
        // if we're not playing any songs, the first song of a batch will never be loudnormed, since this is slow
        // let has_current_song = { self.current_song.lock().await.is_some() };
        // if let (true, Some(work)) = (has_current_song, &mut songs[0].1) {
        //     work.stream_type = StreamType::Online
        // }
        self.queue.push(songs, queue_position).await?;
        Ok(())
    }

    pub async fn add_recommended_songs(&self, query: &str, amount: usize) -> anyhow::Result<()> {
        let songs = song_recommender(query, amount, *self.current_stream_type.lock().await).await?;
        self.queue.push(songs, QueuePosition::default()).await?;
        Ok(())
    }

    pub async fn extend_songs(&self, query: &str, extend_ratio: f64) -> anyhow::Result<()> {
        let mut songs = process_query(query, *self.current_stream_type.lock().await).await?;
        let recommended_songs = song_recommender(
            query,
            (songs.len() as f64 * extend_ratio) as usize,
            *self.current_stream_type.lock().await,
        )
        .await?;
        songs.extend(recommended_songs);
        songs.shuffle(&mut rand::thread_rng());
        self.queue.push(songs, QueuePosition::default()).await?;
        Ok(())
    }

    pub async fn send_track_command<F: Fn(&TrackHandle) -> TrackResult<()>>(
        &self,
        cmd: F,
    ) -> anyhow::Result<()> {
        let track_handle = self.track_handle.lock().await;
        match track_handle.as_ref() {
            Some(track_handle) => cmd(track_handle).map_err(|e| anyhow!(e.to_string())),
            None => Err(anyhow!("no song currently playing")),
        }
    }

    pub async fn pause_resume(&self, try_pause: Option<bool>) -> anyhow::Result<()> {
        // if None, then we reverse the current play/pause state
        let try_pause = try_pause.unwrap_or(!self.is_paused.fetch_xor(true, Ordering::Relaxed));
        // not paused previously
        if try_pause {
            self.send_track_command(TrackHandle::pause).await
        } else {
            self.send_track_command(TrackHandle::play).await
        }
    }

    pub async fn shuffle(&self) -> anyhow::Result<()> {
        self.queue.shuffle().await
    }

    pub async fn clear(&self) -> anyhow::Result<()> {
        self.queue.clear().await
    }

    // on success, returns a bool that specifies whether the queue is now being looped
    pub async fn change_looping(&self, force_looping: Option<bool>) -> anyhow::Result<bool> {
        let mut is_looping = self.is_looping.lock().await;
        *is_looping = force_looping.unwrap_or(!*is_looping);
        Ok(*is_looping)
    }

    pub async fn change_stream_type(&self, stream_type: StreamType) {
        *self.current_stream_type.lock().await = stream_type
    }

    pub async fn cleanup(&self) -> anyhow::Result<()> {
        self.queue.cleanup().await?;
        Ok(())
    }

    pub async fn get_string(&self) -> String {
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
        println!("song ended, {:?}", SystemTime::now());
        let mut current_song = self.audio_state.current_song.lock().await;

        let is_looping = { *self.audio_state.is_looping.lock().await };
        if is_looping {
            // this is sound because we are guaranteed that the current song is in the Ready state
            *self.audio_state.next_looping_song_to_play.lock().await = current_song.take()
        }

        *current_song = None;
        let mut track_handle = self.audio_state.track_handle.lock().await;
        *track_handle = None;

        None
    }
}
