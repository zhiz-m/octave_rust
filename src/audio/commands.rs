use super::{
    audio_state::AudioState,
    types::{self, QueuePosition},
};
use anyhow::{anyhow, Context};
use poise::{serenity_prelude::CacheHttp, ChoiceParameter, Command};
use songbird::tracks::TrackHandle;
use std::sync::Arc;

use crate::{util::send_embed, Data, Error, PoiseContext};

async fn get_audio_state(ctx: &PoiseContext<'_>) -> anyhow::Result<Arc<AudioState>> {
    // let ctx = Arc::new(ctx.clone());
    let user_id = ctx.author().id;
    let guild_id = ctx.guild_id().context("failed to get guild id")?;
    let channel_id = ctx
        .guild()
        .context("failed to get guild")?
        .voice_states
        .get(&user_id)
        .and_then(|voice_state| voice_state.channel_id);
    let channel_id = match channel_id {
        Some(channel_id) => channel_id,
        None => {
            send_embed(
                ctx.http(),
                ctx.channel_id(),
                "Error: please be in a voice channel",
            )
            .await?;
            return Err(anyhow!("Error: please be in a voice channel"));
        }
    };
    let manager = songbird::get(ctx.serenity_context()).await.unwrap();

    let audio_state = {
        let audio_states = ctx.data().audio_states.lock().await;

        audio_states.get(&guild_id).cloned()
    };
    match audio_state {
        Some(state) => {
            state.set_context(ctx).await;
            if let Some(handler) = manager.get(guild_id) {
                let call = handler.lock().await;
                if call.current_channel() != Some(channel_id.into()) {
                    let _ = manager.join(guild_id, channel_id).await?;
                }
            }

            Ok(state)
        }
        None => {
            let handle_lock = manager.join(guild_id, channel_id).await?;
            let audio_state = AudioState::new(handle_lock, ctx);
            {
                let mut audio_states = ctx.data().audio_states.lock().await;
                audio_states.insert(guild_id, audio_state.clone());
            }
            Ok(audio_state)
        }
    }
}

async fn remove_audio_state(ctx: &PoiseContext<'_>) -> anyhow::Result<(), Error> {
    let guild_id = ctx.guild_id().context("failed to get guild id")?;

    let mut audio_states = ctx.data().audio_states.lock().await;

    let state = audio_states
        .remove(&guild_id)
        .context("bot not active in current guild")?;
    state.cleanup().await?;
    Ok(())
}

/// Starts the bot and shows the user interface menu
#[poise::command(prefix_command, slash_command)]
async fn start(ctx: PoiseContext<'_>) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Disconnects the bot
#[poise::command(prefix_command, slash_command)]
async fn exit(ctx: PoiseContext<'_>) -> anyhow::Result<(), Error> {
    remove_audio_state(&ctx).await?;
    send_embed(
        ctx.serenity_context().http(),
        ctx.channel_id(),
        "Disconnected",
    )
    .await?;
    Ok(())
}

#[derive(Copy, Clone, ChoiceParameter)]
enum StreamType {
    Online,
    Loudnorm,
}

impl From<StreamType> for types::StreamType {
    fn from(val: StreamType) -> Self {
        match val {
            StreamType::Online => types::StreamType::Online,
            StreamType::Loudnorm => types::StreamType::Loudnorm,
        }
    }
}
/// Play a song or playlist
#[poise::command(prefix_command, slash_command)]
async fn play(
    ctx: PoiseContext<'_>,
    #[description = "shuffle songs?"] shuffle: bool,
    #[description = "normalize volume?"] loudnorm: bool,
    #[description = "song/playlist URL or search query"] query: String,
) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    let loudnorm = match loudnorm {
        true => types::StreamType::Loudnorm,
        false => types::StreamType::Online,
    };
    audio_state
        .add_audio(&query, QueuePosition::default(), shuffle, loudnorm)
        .await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Use our advanced song recommendation algorithm to play songs
#[poise::command(prefix_command, slash_command)]
async fn recommend(
    ctx: PoiseContext<'_>,
    #[description = "Spotify playlist link"] query: String,
    #[description = "number of songs"] amount: String,
) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    let amount = amount.parse().context("invalid integer")?;
    audio_state.add_recommended_songs(&query, amount).await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Use our advanced song recommendation algorithm to play songs in addition to your playlist
#[poise::command(prefix_command, slash_command)]
async fn extend(
    ctx: PoiseContext<'_>,
    #[description = "Spotify playlist link"] query: String,
    #[description = "ratio of recommended songs to add"] ratio: String,
) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;

    let extend_ratio = ratio.parse().context("invalid ratio")?;
    audio_state.extend_songs(&query, extend_ratio).await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Skips the currently playing song
#[poise::command(prefix_command, slash_command)]
async fn skip(ctx: PoiseContext<'_>) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state.send_track_command(TrackHandle::stop).await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Play or pause the audio player
#[poise::command(prefix_command, slash_command)]
async fn pause_resume(
    ctx: PoiseContext<'_>,
    #[description = "Whether to pause (y) or resume (n)"] b: bool,
) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state.pause_resume(Some(b)).await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Shuffles the order of queued songs
#[poise::command(prefix_command, slash_command)]
async fn shuffle(ctx: PoiseContext<'_>) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state.shuffle().await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Removes all queued songs
#[poise::command(prefix_command, slash_command)]
async fn clear(ctx: PoiseContext<'_>) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state.clear().await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Sets the current song to loop / not loop
#[poise::command(prefix_command, slash_command)]
async fn looping(
    ctx: PoiseContext<'_>,
    #[description = "Whether to loop (y) or not loop (n)"] b: bool,
) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state.change_looping(Some(b)).await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Changes the stream type: allowed values are "online" or "loudnorm"
#[poise::command(prefix_command, slash_command)]
async fn stream_type(
    ctx: PoiseContext<'_>,
    #[description = "Allowed values: \"online\" or \"loudnorm\" "] query: StreamType,
) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state.change_stream_type(query.into()).await;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    Ok(())
}

/// Displays the queue
#[poise::command(prefix_command, slash_command)]
async fn queue(ctx: PoiseContext<'_>) -> anyhow::Result<(), Error> {
    let audio_state = get_audio_state(&ctx).await?;
    audio_state
        .display_ui_with_poise_context_reply(&ctx)
        .await?;
    send_embed(
        ctx.serenity_context().http(),
        ctx.channel_id(),
        &audio_state.get_string().await,
    )
    .await?;
    Ok(())
}

pub fn add_group(commands: &mut Vec<Command<Data, Error>>) {
    commands.extend(vec![
        start(),
        exit(),
        play(),
        recommend(),
        extend(),
        skip(),
        pause_resume(),
        shuffle(),
        clear(),
        looping(),
        stream_type(),
        queue(),
    ])
}
