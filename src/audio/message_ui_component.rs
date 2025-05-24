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
        CreateInputText, CreateSelectMenu, CreateSelectMenuOption, InputTextStyle, Message, UserId,
    },
    CreateReply,
};
use serenity::all::{
    ComponentInteraction, ComponentInteractionDataKind, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage, CreateModal, CreateSelectMenuKind, InputText,
    ModalInteraction,
};
use songbird::tracks::TrackHandle;
use tokio::{sync::Mutex, time::timeout};

use super::{
    audio_state::AudioState,
    db::Db,
    types::{QueuePosition, StreamType},
};

#[derive(Clone, Copy)]
struct UserState {
    should_shuffle: bool,
    stream_type: StreamType,
    queue_position: QueuePosition,
}

impl Default for UserState {
    fn default() -> Self {
        Self {
            should_shuffle: true,
            stream_type: StreamType::Loudnorm,
            queue_position: QueuePosition::default(),
        }
    }
}

pub struct MessageUiComponent {
    should_cleanup: Arc<AtomicBool>,
    context: Arc<Context>,
    audio_state: Arc<AudioState>,
    user_state_map: Arc<Mutex<HashMap<UserId, UserState>>>,
}

fn parse_db_buttom_id(id: &str) -> Option<&str> {
    id.strip_prefix("add_songs_from_db_")
}

fn create_db_buttom_id(db_key: &str) -> String {
    format!("add_songs_from_db_{db_key}")
}

fn now_playing_response(query: &str) -> CreateInteractionResponse {
    CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .add_embed(get_styled_embed(&format!("***Now playing from:*** _{query}_")).to_owned()),
    )
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

    async fn components(context: &Arc<Context>) -> Vec<CreateActionRow> {
        let mut all = vec![
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
            // todo: renable when we can hjave more than 5 rows
            // CreateActionRow::SelectMenu(CreateSelectMenu::new(
            //     "enable_loudnorm",
            //     CreateSelectMenuKind::String {
            //         options: vec![
            //             CreateSelectMenuOption::new("Enable volume normalization: yes", "t")
            //                 .default_selection(true)
            //                 .to_owned(),
            //             CreateSelectMenuOption::new("Enable volume normalization: no", "f")
            //                 .default_selection(false)
            //                 .to_owned(),
            //         ],
            //     },
            // )),
            CreateActionRow::SelectMenu(CreateSelectMenu::new(
                "queue_position",
                CreateSelectMenuKind::String {
                    options: vec![
                        CreateSelectMenuOption::new("Initial queue position: front", "t")
                            .default_selection(true)
                            .to_owned(),
                        CreateSelectMenuOption::new("Initial queue position: back", "f")
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
        ];

        {
            let mut data = context.data.write().await;
            let data = data
                .get_mut::<Db>()
                .context("Db object was not initialized in serenity TypeMap")
                .unwrap()
                .data();
            if !data.is_empty() {
                let buttons = data
                    .keys()
                    .map(|key| {
                        CreateButton::new(create_db_buttom_id(key))
                            .emoji('🎶')
                            .style(ButtonStyle::Primary)
                            .label(key)
                            .to_owned()
                    })
                    .collect();
                all.push(CreateActionRow::Buttons(buttons))
            }
        };
        all
    }

    pub async fn start_with_channel_id(&mut self, channel_id: ChannelId) -> anyhow::Result<()> {
        let m = channel_id
            .send_message(
                self.context.http.clone(),
                CreateMessage::new().components(Self::components(&self.context).await),
            )
            .await?;

        self.init_handler(m);
        Ok(())
    }

    pub async fn start_with_poise_context(&mut self, ctx: &PoiseContext<'_>) -> anyhow::Result<()> {
        let handle = ctx
            .send(CreateReply::default().components(Self::components(&self.context).await))
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
                mci.defer(&context.http).await?;
            }
            "clear" => {
                audio_state.clear().await?;
                mci.defer(&context.http).await?;
            }
            "loop" => {
                let is_looping = audio_state.change_looping(None).await?;
                mci.create_response(
                    &context.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().add_embed(
                            get_styled_embed(&format!("Looping: {is_looping}")).to_owned(),
                        ),
                    ),
                )
                .await?
            }
            "play_pause" => {
                audio_state.pause_resume(None).await?;
                mci.defer(&context.http).await?;
            }
            "shuffle_selection" => {
                let selections = match &mci.data.kind {
                    ComponentInteractionDataKind::StringSelect { values } => values,
                    other => panic!("unexpected selection {other:#?}"),
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
            "enable_loudnorm" => {
                let selections = match &mci.data.kind {
                    ComponentInteractionDataKind::StringSelect { values } => values,
                    other => panic!("unexpected selection {other:#?}"),
                };
                let user_id = mci.user.id;
                let mut user_state_map = user_state_map.lock().await;
                let user_state = match user_state_map.entry(user_id) {
                    Entry::Occupied(e) => e.into_mut(),
                    Entry::Vacant(e) => e.insert(UserState::default()),
                };
                match selections.iter().next().context("no selections")?.as_str() {
                    "t" => user_state.stream_type = StreamType::Loudnorm,
                    "f" => user_state.stream_type = StreamType::Online,
                    _ => unreachable!(),
                };
                mci.defer(context).await?;
            }
            "queue_position" => {
                let selections = match &mci.data.kind {
                    ComponentInteractionDataKind::StringSelect { values } => values,
                    other => panic!("unexpected selection {other:#?}"),
                };
                let user_id = mci.user.id;
                let mut user_state_map = user_state_map.lock().await;
                let user_state = match user_state_map.entry(user_id) {
                    Entry::Occupied(e) => e.into_mut(),
                    Entry::Vacant(e) => e.insert(UserState::default()),
                };
                match selections.iter().next().context("no selections")?.as_str() {
                    "t" => user_state.queue_position = QueuePosition::Front,
                    "f" => user_state.queue_position = QueuePosition::Back,
                    _ => unreachable!(),
                };
                mci.defer(context).await?;
            }
            "add_songs" => {
                let components = vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Paragraph, "Song query", "song_query")
                            .placeholder(
                                "spotify playlist/track/album URL, or youtube track/playlist URL",
                            )
                            .min_length(1)
                            .max_length(300)
                            .to_owned(),
                    ),
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "Save DB key", "db_key")
                            .placeholder("(ignored if blank)")
                            .min_length(0)
                            .max_length(50)
                            .required(false)
                            .to_owned(),
                    ),
                ];
                mci.create_response(
                    context,
                    CreateInteractionResponse::Modal(
                        CreateModal::new("song_query_modal", "Song query").components(components),
                    ),
                )
                .await?;
            }
            "queue" => {
                let text = audio_state.get_string().await;
                mci.create_response(
                    &context.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .add_embed(get_styled_embed(&text).to_owned()),
                    ),
                )
                .await?;
                audio_state.display_ui().await?;
            }
            db_key_id if parse_db_buttom_id(db_key_id).is_some() => {
                let query = {
                    let db_key = parse_db_buttom_id(db_key_id).unwrap();
                    let mut data = context.data.write().await;
                    let data = data
                        .get_mut::<Db>()
                        .context("Db object was not initialized in serenity TypeMap")
                        .unwrap()
                        .data();
                    data.get(db_key)
                        .context("failed to find db key inside database")?
                        .clone()
                };

                let user_state_map = user_state_map.lock().await;
                let user_state = user_state_map
                    .get(&mci.user.id)
                    .cloned()
                    .unwrap_or_default();
                audio_state
                    .add_audio(
                        &query,
                        user_state.queue_position,
                        user_state.should_shuffle,
                        user_state.stream_type,
                    )
                    .await?;
                mci.create_response(context, now_playing_response(&query))
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
        let components: Vec<_> = mci
            .data
            .components
            .iter()
            .flat_map(|x| x.components.iter())
            .filter_map(|component| match component {
                ActionRowComponent::InputText(x) => Some(x),
                _ => None,
            })
            .collect();
        let find = |key: &'static str| {
            move |x: &&InputText| match x.custom_id == key {
                true => x.value.clone(),
                false => None,
            }
        };

        let query = components
            .iter()
            .find_map(find("song_query"))
            .context("process_modal_interaction: no song_query field")?;
        let db_key = components
            .iter()
            .find_map(find("db_key"))
            .context("process_modal_interaction: no db_key field")?;
        let user_id = mci.user.id;
        let user_state_map = user_state_map.lock().await;
        let user_state = user_state_map.get(&user_id).cloned().unwrap_or_default();
        audio_state
            .add_audio(
                &query,
                user_state.queue_position,
                user_state.should_shuffle,
                user_state.stream_type,
            )
            .await?;
        match db_key.is_empty() {
            true => (),
            false => {
                let mut data = context.data.write().await;
                let db = data
                    .get_mut::<Db>()
                    .context("Db object was not initialized in serenity TypeMap")
                    .unwrap();
                db.insert_and_flush(db_key.clone(), query.clone())?
            }
        };
        mci.create_response(context, now_playing_response(&query))
            .await?;
        audio_state.display_ui().await?;
        Ok(())
    }
}

impl Drop for MessageUiComponent {
    fn drop(&mut self) {
        self.should_cleanup.swap(true, Ordering::Relaxed);
    }
}
