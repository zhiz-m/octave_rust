use std::{sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

use serenity::{model::prelude::{Message, ChannelId, component::ButtonStyle, interaction::{message_component::MessageComponentInteraction, InteractionResponseType}}, prelude::Context, builder::{CreateActionRow, CreateButton}};
use songbird::tracks::TrackCommand;
use crate::config::audio::MESSAGE_UI_COMPONENT_PROCESING_INTERVAL_MS;

use super::audio_state::AudioState;
pub struct MessageUiComponent{
    should_cleanup: Arc<AtomicBool>,
    message: Option<Message>,
    channel_id: ChannelId,
    context: Arc<Context>,
    audio_state: Arc<AudioState>,
}

impl MessageUiComponent{
    pub fn new(audio_state: Arc<AudioState>, channel_id: ChannelId, context: Arc<Context>) -> MessageUiComponent {
        MessageUiComponent { 
            should_cleanup: Arc::new(AtomicBool::new(false)),
            message: None,
            channel_id,
            context,
            audio_state,
        }
    }

    pub async fn start(&mut self, text: String) -> Result<(), String>{
        self.channel_id.send_message(self.context.http.clone(), |m| {
            m.embed(|e| {
                e.colour(0xf542bf);
                e.description(&format!(
                    "Now playing:\n\n {}", text
                ));
                e
            })
        }).await.map_err(|e| e.to_string())?;
        
        let m = self.channel_id.send_message(self.context.http.clone(), |m| {
            m.components(|c| 
                    c
                    .add_action_row(
                        CreateActionRow::default()
                            .add_button(
                                CreateButton::default()
                                    .custom_id("skip")
                                    .emoji('↪')
                                    .style(ButtonStyle::Secondary)
                                    .label("Skip")
                                    .to_owned()
                            )
                            .add_button(
                                CreateButton::default()
                                    .custom_id("clear")
                                    .emoji('🗑')
                                    .style(ButtonStyle::Danger)
                                    .label("Clear")
                                    .to_owned()
                            ).to_owned()
                    )
                    .add_action_row(
                        CreateActionRow::default()
                            .add_button(
                                CreateButton::default()
                                    .custom_id("play_pause")
                                    .emoji('⏸')
                                    .emoji('▶')
                                    .style(ButtonStyle::Secondary)
                                    .label("Play/Pause")
                                    .to_owned()
                            ).to_owned()
                    )
                )
        }).await.map_err(|e| e.to_string())?;

        self.message = Some(m);
        self.interaction_loop();
        Ok(())
    }

    fn interaction_loop(&mut self) {
        let m = match self.message.as_ref(){
            Some(m) => m.clone(),
            None => {
                println!("error in interaction_loop: ui message is empty");
                return;
            }
        };
        let context = self.context.clone();
        let should_cleanup = self.should_cleanup.clone();
        let audio_state = self.audio_state.clone();
        tokio::spawn(async move {
            loop{
                if let Some(mci) = m.await_component_interaction(&context.shard).timeout(Duration::from_millis(MESSAGE_UI_COMPONENT_PROCESING_INTERVAL_MS)).await {
                    if let Err(why) = Self::process_interaction(mci.clone(), audio_state.clone()).await.map_err(|e| e.to_string()){
                        println!("error in interaction_loop: {}", why);
                    }
                    if let Err(why) = mci.create_interaction_response(&context, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    }).await {
                        println!("error in interaction_loop: {}", why);
                    }
                };
                if should_cleanup.swap(false, Ordering::Relaxed){
                    break;
                }
            }
            if let Err(why) = m.delete(&context.http).await{
                println!("error in interaction_loop: {}", why);
            };
        });
    }

    async fn process_interaction(mci: Arc<MessageComponentInteraction>, audio_state: Arc<AudioState>) -> Result<(), String>{
        let id = mci.data.custom_id.as_str();

        match id {
            "skip" => {
                audio_state.send_track_command(TrackCommand::Stop).await?;
            },
            "clear" => {
                audio_state.clear().await?
            },
            "play_pause" => {
                audio_state.pause_resume().await?;
            },
            _ => unreachable!()
        };
        Ok(())
    }
}

impl Drop for MessageUiComponent {
    fn drop(&mut self) {
        self.should_cleanup.swap(true, Ordering::Relaxed);
    }
}