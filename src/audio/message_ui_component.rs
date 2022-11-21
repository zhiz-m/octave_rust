use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::{config::audio::MESSAGE_UI_COMPONENT_CHAIN_INTERVAL_MS, util::get_styled_embed};
use anyhow::Context as AContext;
use futures::StreamExt;
use poise::serenity_prelude::{Message, ChannelId, Context, UserId, CreateActionRow, CreateButton, ButtonStyle, CreateSelectMenu, CreateSelectMenuOption, MessageComponentInteraction, InteractionResponseType, ModalSubmitInteraction, ActionRowComponent, CreateEmbed, CreateInputText, InputTextStyle};
use songbird::tracks::TrackCommand;
use tokio::{sync::Mutex, time::timeout};

use super::audio_state::AudioState;

struct UserState {
    should_shuffle: bool,
}

impl Default for UserState {
    fn default() -> Self {
        Self {
            should_shuffle: true,
        }
    }
}

pub struct MessageUiComponent {
    should_cleanup: Arc<AtomicBool>,
    message: Option<Message>,
    channel_id: ChannelId,
    context: Arc<Context>,
    audio_state: Arc<AudioState>,
    user_state_map: Arc<Mutex<HashMap<UserId, UserState>>>,
}

impl MessageUiComponent {
    pub fn new(audio_state: Arc<AudioState>, channel_id: ChannelId, context: Arc<Context>) -> Self {
        Self {
            should_cleanup: Arc::new(AtomicBool::new(false)),
            message: None,
            channel_id,
            context,
            audio_state,
            user_state_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&mut self) -> Result<(), String> {
        let m = self
            .channel_id
            .send_message(self.context.http.clone(), |m| {
                m.components(|c| {
                    c.add_action_row(
                        CreateActionRow::default()
                            .add_button(
                                CreateButton::default()
                                    .custom_id("skip")
                                    .emoji('↪')
                                    .style(ButtonStyle::Secondary)
                                    .label("Skip")
                                    .to_owned(),
                            )
                            .add_button(
                                CreateButton::default()
                                    .custom_id("clear")
                                    .emoji('🗑')
                                    .style(ButtonStyle::Danger)
                                    .label("Clear")
                                    .to_owned(),
                            )
                            .add_button(
                                CreateButton::default()
                                    .custom_id("play_pause")
                                    .emoji('⏸')
                                    .emoji('▶')
                                    .style(ButtonStyle::Secondary)
                                    .label("Play/Pause")
                                    .to_owned(),
                            )
                            .add_button(
                                CreateButton::default()
                                    .custom_id("loop")
                                    .emoji('🔁')
                                    .style(ButtonStyle::Secondary)
                                    .label("Loop")
                                    .to_owned(),
                            )
                            .to_owned(),
                    )
                    .add_action_row(
                        CreateActionRow::default()
                            .add_select_menu(
                                CreateSelectMenu::default()
                                    .custom_id("shuffle_selection")
                                    .options(|m| {
                                        m.add_option(
                                            CreateSelectMenuOption::default()
                                                .default_selection(true)
                                                .value("t")
                                                .label("Shuffle newly added songs: yes")
                                                .to_owned(),
                                        )
                                        .add_option(
                                            CreateSelectMenuOption::default()
                                                .default_selection(false)
                                                .value("f")
                                                .label("Shuffle newly added songs: no")
                                                .to_owned(),
                                        )
                                    })
                                    .to_owned(),
                            )
                            .to_owned(),
                    )
                    .add_action_row(
                        CreateActionRow::default()
                            .add_button(
                                CreateButton::default()
                                    .custom_id("add_songs")
                                    .emoji('🎶')
                                    .style(ButtonStyle::Success)
                                    .label("Add Songs")
                                    .to_owned(),
                            )
                            .add_button(
                                CreateButton::default()
                                    .custom_id("queue")
                                    .emoji('🗒')
                                    .style(ButtonStyle::Secondary)
                                    .label("Display Queue")
                                    .to_owned(),
                            )
                            .to_owned(),
                    )
                })
            })
            .await
            .map_err(|e| e.to_string())?;

        self.message = Some(m);
        self.initiate_tasks();
        Ok(())
    }

    fn initiate_tasks(&mut self) {
        let m = match self.message.as_ref() {
            Some(m) => Arc::new(m.clone()),
            None => {
                log::error!("error in interaction_loop: ui message is empty");
                return;
            }
        };

        {
            let context = self.context.clone();
            let should_cleanup = self.should_cleanup.clone();
            let audio_state = self.audio_state.clone();
            let user_state_map = self.user_state_map.clone();
            let m = m.clone();
            tokio::spawn(async move {
                // we will stop waiting for interactions after 1 hour
                let mut mci_iter = m
                    .await_component_interactions(&context.shard)
                    .timeout(Duration::from_secs(3600))
                    .build();
                loop {
                    while let Ok(Some(mci)) = timeout(
                        Duration::from_millis(MESSAGE_UI_COMPONENT_CHAIN_INTERVAL_MS),
                        mci_iter.next(),
                    )
                    .await
                    {
                        if let Err(why) = Self::process_message_interaction(
                            &mci,
                            &context,
                            &user_state_map,
                            &audio_state,
                        )
                        .await
                        {
                            log::error!("error in interaction_loop: {}", why);
                        }
                    }

                    if should_cleanup.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if let Err(why) = m.delete(&context.http).await {
                    log::error!("error in interaction_loop: {}", why);
                };
            });
        }

        {
            let context = self.context.clone();
            let should_cleanup = self.should_cleanup.clone();
            let audio_state = self.audio_state.clone();
            let user_state_map = self.user_state_map.clone();
            tokio::spawn(async move {
                // we will stop waiting for interactions after 1 hour
                let mut mci_iter = m
                    .await_modal_interactions(&context.shard)
                    .timeout(Duration::from_secs(3600))
                    .build();
                loop {
                    while let Ok(Some(mci)) = timeout(
                        Duration::from_millis(MESSAGE_UI_COMPONENT_CHAIN_INTERVAL_MS),
                        mci_iter.next(),
                    )
                    .await
                    {
                        if let Err(why) = Self::process_modal_interaction(
                            &mci,
                            &context,
                            &user_state_map,
                            &audio_state,
                        )
                        .await
                        {
                            log::error!("error in interaction_loop: {}", why);
                        }
                    }
                    if should_cleanup.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    }

    async fn process_message_interaction(
        mci: &Arc<MessageComponentInteraction>,
        context: &Arc<Context>,
        user_state_map: &Arc<Mutex<HashMap<UserId, UserState>>>,
        audio_state: &Arc<AudioState>,
    ) -> anyhow::Result<()> {
        let id = mci.data.custom_id.as_str();

        match id {
            "skip" => {
                audio_state.send_track_command(TrackCommand::Stop).await?;
                mci
                    .create_interaction_response(context, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
            }
            "clear" => {
                audio_state.clear().await?;
                mci
                    .create_interaction_response(context, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
            }
            "loop" => {
                audio_state.change_looping(None).await?;
                mci
                    .create_interaction_response(context, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
            }
            "play_pause" => {
                audio_state.pause_resume(None).await?;
                mci
                    .create_interaction_response(context, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
            }
            "shuffle_selection" => {
                let selections = &mci.data.values;
                let user_id = mci.user.id;
                let mut user_state_map = user_state_map.lock().await;
                let user_state = match user_state_map.entry(user_id) {
                    Entry::Occupied(e) => e.into_mut(),
                    Entry::Vacant(e) => e.insert(UserState::default()),
                };
                match selections.iter().next().context("no selections")?.as_str() {
                    "t" => user_state.should_shuffle = true,
                    "f" => user_state.should_shuffle = false,
                    _ => unreachable!(),
                };
                mci
                    .create_interaction_response(context, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
            }
            "add_songs" => {
                mci.create_interaction_response(context, |r| {
                    r.kind(InteractionResponseType::Modal)
                        .interaction_response_data(|d| {
                            d
                            .custom_id("song_query_modal")
                            .title("Song query")
                            .add_embed(get_styled_embed(&mut CreateEmbed::default(), "test").to_owned())
                            .components(|c| {
                                c.add_action_row(CreateActionRow::default()
                                    .add_input_text(
                                        CreateInputText::default()
                                            .custom_id("song_query")
                                            .style(InputTextStyle::Paragraph)
                                            .label("Song query")
                                            .placeholder("eg https://open.spotify.com/playlist/XXXXXXXXX or https://www.youtube.com/watch?v=XXXXXXX")
                                            .min_length(1)
                                            .max_length(300)
                                            .to_owned()
                                    ).to_owned()
                                )
                            })
                        })
                }).await?;
            }
            "queue" => {
                let text = audio_state.get_string().await;
                mci
                    .create_interaction_response(context, |r| {
                        r.kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(move |d| {
                                d.add_embed(
                                    get_styled_embed(&mut CreateEmbed::default(), &text).to_owned(),
                                )
                            })
                    })
                    .await?;
                audio_state.display_ui().await;
            }
            _ => unreachable!(),
        };
        Ok(())
    }

    async fn process_modal_interaction(
        mci: &Arc<ModalSubmitInteraction>,
        context: &Arc<Context>,
        user_state_map: &Arc<Mutex<HashMap<UserId, UserState>>>,
        audio_state: &Arc<AudioState>,
    ) -> Result<(), String> {
        let id = mci.data.custom_id.as_str();
        if id != "song_query_modal" {
            return Err("process_modal_interaction: id was not \"song_query_modal\"".to_owned());
        }
        let component = mci
            .data
            .components
            .iter()
            .flat_map(|x| x.components.iter())
            .next()
            .ok_or("process_modal_interaction: no ActionRowComponent")?;
        let query = match component {
            ActionRowComponent::InputText(x) => &x.value,
            _ => return Err("process_modal_interaction: no InputText component".to_owned()),
        };
        let user_id = mci.user.id;
        let user_state_map = user_state_map.lock().await;
        let shuffle = match user_state_map.get(&user_id) {
            Some(state) => state.should_shuffle,
            None => true,
        };
        audio_state.add_audio(query, shuffle).await;
        mci.create_interaction_response(context, |r| {
            r.kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|d| {
                    d.content(&format!("***Now playing from:*** _{}_", query))
                })
        })
        .await
        .map_err(|e| e.to_string())?;
        audio_state.display_ui().await;
        Ok(())
    }
}

impl Drop for MessageUiComponent {
    fn drop(&mut self) {
        self.should_cleanup.swap(true, Ordering::Relaxed);
    }
}
