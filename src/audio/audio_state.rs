use std::{
    sync::Arc,
};
use super::{
    song_queue::SongQueue,
    song::{
        Song,
        SongMetadata,
    },
};
use songbird::{
    Call,
    input,
    EventHandler as VoiceEventHandler,
    Event,
    EventContext,
    TrackEvent,
};
use serenity::{
    async_trait,
    prelude::{
        Mutex as SerenityMutex,
    }
};
pub struct AudioState{
    queue: SongQueue,
    handler: Arc<SerenityMutex<Call>>,
}

impl AudioState{
    pub fn new(handler: Arc<SerenityMutex<Call>>) -> Arc<AudioState>{
        let audio_state = AudioState{
            queue: SongQueue::new(),
            handler: handler.clone(),
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
        let source = input::ffmpeg_optioned(url, &[
            "-reconnect","1",
            "-reconnect_streamed","1",
            "-reconnect_delay_max","5",
        ], &[
            "-f","s16le",
            "-ar","48000",
            "-ac","2",
            "-acodec","pcm_f32le",
            "-"
        ]);
        let source = match source.await {
            Ok(source) => source,
            Err(why) => {
                panic!("Err AudioState::play_audio: {:?}", why);
            }
        };
        
        let mut handler = audio_state.handler.lock().await;
        let song = handler.play_source(source);

        if let Err(why) = song.add_event(
            Event::Track(TrackEvent::End),
            SongEndNotifier{
                audio_state: audio_state.clone(),
            }
        ){
            panic!("Err AudioState::play_audio: {:?}", why);
        }
    }

    pub async fn add_audio(audio_state: Arc<AudioState>, query: &str){
        let metadata = SongMetadata{
            artist: None,
            title: None,
            duration: None,
            query: Some(query.to_string()),
        };
        let song_work = Song::new_load(metadata).unwrap();
        println!("hi1");
        audio_state.queue.push(vec![song_work]).await;
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
        AudioState::play_audio(self.audio_state.clone()).await;

        None
    }
}