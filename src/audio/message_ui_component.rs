use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::{
    config::audio::MESSAGE_UI_COMPONENT_CHAIN_INTERVAL_MS, util::get_styled_embed, PoiseContext,
};
use anyhow::{anyhow, Context as AContext};
use futures::StreamExt;
use poise::{
    serenity_prelude::{
        ActionRowComponent, ButtonStyle, ChannelId, Context, CreateActionRow, CreateButton,
        CreateInputText, CreateSelectMenu, CreateSelectMenuOption, InputTextStyle,
        Message, UserId,
    },
    CreateReply,
};
use serenity::all::{
    ComponentInteraction, ComponentInteractionDataKind, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage, CreateModal, CreateSelectMenuKind,
    ModalInteraction,
};
use songbird::tracks::TrackHandle;
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
    context: Arc<Context>,
    audio_state: Arc<AudioState>,
    user_state_map: Arc<Mutex<HashMap<UserId, UserState>>>,
}

impl MessageUiComponent {
    pub fn new(audio_state: Arc<AudioState>, context: Arc<Context>) -> Self {
        Self {
            should_cleanup: Arc::new(AtomicBool::new(false)),
            context,
            audio_state,
            user_state_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn components() -> Vec<CreateActionRow> {
        vec![
            CreateActionRow::Buttons(vec![
                CreateButton::new("skip")
                    .emoji('↪')
                    .style(ButtonStyle::Secondary)
                    .label("Skip")
                    .to_owned(),
                CreateButton::new("clear")
                    .emoji('🗑')
                    .style(ButtonStyle::Danger)
                    .label("Clear")
                    .to_owned(),
                CreateButton::new("play_pause")
                    .emoji('⏸')
                    .emoji('▶')
                    .style(ButtonStyle::Secondary)
                    .label("Play/Pause")
                    .to_owned(),
                CreateButton::new("loop")
                    .emoji('🔁')
                    .style(ButtonStyle::Secondary)
                    .label("Loop")
                    .to_owned(),
            ]),
            CreateActionRow::SelectMenu(CreateSelectMenu::new(
                "shuffle_selection",
                CreateSelectMenuKind::String {
                    options: vec![
                        CreateSelectMenuOption::new("Shuffle newly added songs: yes", "t")
                            .default_selection(true)
                            .to_owned(),
                        CreateSelectMenuOption::new("Shuffle newly added songs: no", "f")
                            .default_selection(false)
                            .to_owned(),
                    ],
                },
            )),
            CreateActionRow::Buttons(vec![
                CreateButton::new("add_songs")
                    .emoji('🎶')
                    .style(ButtonStyle::Success)
                    .label("Add Songs")
                    .to_owned(),
                CreateButton::new("queue")
                    .emoji('🗒')
                    .style(ButtonStyle::Secondary)
                    .label("Display Queue")
                    .to_owned(),
            ]),
        ]
    }

    pub async fn start_with_channel_id(&mut self, channel_id: ChannelId) -> anyhow::Result<()> {
        let m = channel_id
            .send_message(
                self.context.http.clone(),
                CreateMessage::new().components(Self::components()),
            )
            .await?;

        self.init_handler(m);
        Ok(())
    }

    pub async fn start_with_poise_context(&mut self, ctx: &PoiseContext<'_>) -> anyhow::Result<()> {
        let handle = ctx
            .send(CreateReply::default().components(Self::components()))
            .await?;

        self.init_handler(handle.into_message().await?);
        Ok(())
    }

    fn init_handler(&mut self, m: Message) {
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
                    .stream();
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
                    .stream();
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
        mci: &ComponentInteraction,
        context: &Arc<Context>,
        user_state_map: &Arc<Mutex<HashMap<UserId, UserState>>>,
        audio_state: &Arc<AudioState>,
    ) -> anyhow::Result<()> {
        let id = mci.data.custom_id.as_str();

        match id {
            "skip" => {
                audio_state.send_track_command(TrackHandle::stop).await?;
                //todo maybe defer message
                // mci.create_response(context, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new())).await?;
                mci.defer(&context.http).await?;
            }
            "clear" => {
                audio_state.clear().await?;
                mci.defer(&context.http).await?;
                // mci.create_response(context, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new())).await?;
            }
            "loop" => {
                audio_state.change_looping(None).await?;
                mci.defer(&context.http).await?;
                // mci.create_response(context, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new())).await?;
            }
            "play_pause" => {
                audio_state.pause_resume(None).await?;
                mci.defer(&context.http).await?;
                // mci.create_response(context, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new())).await?;
            }
            "shuffle_selection" => {
                let selections = match &mci.data.kind {
                    ComponentInteractionDataKind::StringSelect { values } => values,
                    other => panic!("unexpected selection {:#?}", other),
                };
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
                  mci.defer(context).await?;
            }
            "add_songs" => {
                mci.create_response(context, CreateInteractionResponse::Modal(
                    CreateModal::new("song_query_modal", "Song query")
                    
                    .components(
                        vec![CreateActionRow::InputText(
                            CreateInputText::new(InputTextStyle::Paragraph, "Song query", "song_query")
                            .placeholder("eg https://open.spotify.com/playlist/XXXXXXXXX or https://www.youtube.com/watch?v=XXXXXXX or arbitrary youtube search query")
                            .min_length(1)
                            .max_length(300)
                            .to_owned()
                        )])
                        
                  )
                ).await?;
            }
            "queue" => {
                let text = audio_state.get_string().await;
                mci.create_response(
                    &context.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().add_embed(
                            get_styled_embed(&text).to_owned(),
                        ),
                    ),
                )
                .await?;
                audio_state.display_ui().await?;
            }
            _ => unreachable!(),
        };
        Ok(())
    }

    async fn process_modal_interaction(
        mci: &ModalInteraction,
        context: &Arc<Context>,
        user_state_map: &Arc<Mutex<HashMap<UserId, UserState>>>,
        audio_state: &Arc<AudioState>,
    ) -> anyhow::Result<()> {
        let id = mci.data.custom_id.as_str();
        if id != "song_query_modal" {
            return Err(anyhow!(
                "process_modal_interaction: id was not \"song_query_modal\""
            ));
        }
        let component = mci
            .data
            .components
            .iter()
            .flat_map(|x| x.components.iter())
            .next()
            .context("process_modal_interaction: no ActionRowComponent")?;
        let query = match component {
            ActionRowComponent::InputText(x) => &x.value,
            _ => return Err(anyhow!("process_modal_interaction: no InputText component")),
        };
        let user_id = mci.user.id;
        let user_state_map = user_state_map.lock().await;
        let shuffle = match user_state_map.get(&user_id) {
            Some(state) => state.should_shuffle,
            None => true,
        };
        if let Some(query) =
            query {
                audio_state.add_audio(query, shuffle).await?;
                mci.create_response(
                    context,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().add_embed(
                            get_styled_embed(
                                &format!("***Now playing from:*** _{}_", query),
                            )
                            .to_owned(),
                        ),
                    ),
                )
                .await?;
                audio_state.display_ui().await?;
            };
        Ok(())
    }
}

impl Drop for MessageUiComponent {
    fn drop(&mut self) {
        self.should_cleanup.swap(true, Ordering::Relaxed);
    }
}
