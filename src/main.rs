use audio::{
    audio_state::AudioState,
    config::{self, audio::BOT_PREFIX},
};
use songbird::SerenityInit;
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::Mutex;
use util::send_embed;

mod audio;
mod logger;
mod util;

use poise::{
    samples::create_application_commands,
    serenity_prelude::{CacheHttp, Command, CreateApplicationCommands, GatewayIntents, GuildId},
    Context as RawPoiseContext,
};

type Error = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = RawPoiseContext<'a, Data, Error>;

pub struct Data {
    pub audio_states: Mutex<HashMap<GuildId, Arc<AudioState>>>,
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, ctx } => {
            if let Err(e) =
                send_embed(ctx.discord().http(), ctx.channel_id(), &error.to_string()).await
            {
                log::error!("Error while sending error embed: {}", e)
            };
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                log::error!("Error while handling error: {}", e)
            }
        }
    }
}

pub fn get_default_guilds() -> Vec<GuildId> {
    if let Ok(guilds) = env::var("OCTAVE_BOT_GUILDS") {
        return guilds
            .split(',')
            .filter_map(|g| Some(GuildId(g.parse().ok()?)))
            .collect();
    }
    vec![]
}

#[tokio::main]
async fn main() {
    logger::init_logger().expect("failed to init logger");
    let mut commands = vec![];
    audio::add_group(&mut commands);
    let cmd_builder = create_application_commands(&commands);
    let options = poise::FrameworkOptions {
        commands,
        on_error: |error| Box::pin(on_error(error)),
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some(BOT_PREFIX.to_owned()),
            mention_as_prefix: false,

            ..Default::default()
        },
        ..Default::default()
    };
    poise::Framework::builder()
        .token(env::var("OCTAVE_BOT_TOKEN").expect("Error: token not found"))
        .options(options)
        .intents(
            GatewayIntents::GUILD_MESSAGES
                | GatewayIntents::GUILD_VOICE_STATES
                | GatewayIntents::MESSAGE_CONTENT
                | GatewayIntents::GUILDS,
        )
        .user_data_setup(|ctx, _, _| {
            Box::pin(async move {
                for guild in ctx.cache.guilds() {
                    guild
                        .set_application_commands(ctx.http(), |commands| {
                            *commands = cmd_builder.clone();
                            commands
                        })
                        .await?;
                }
                Command::set_global_application_commands(ctx.http(), |commands| {
                    *commands = CreateApplicationCommands::default();
                    commands
                })
                .await?;
                //
                Ok(Data {
                    audio_states: Mutex::new(HashMap::new()),
                })
            })
        })
        .client_settings(|b| b.register_songbird())
        .run()
        .await
        .expect("client error");
}
