use audio::{
    audio_state::AudioState,
    config::{self, audio::BOT_PREFIX},
};
use serenity::all::ClientBuilder;
use songbird::SerenityInit;
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::Mutex;
use util::send_embed;

mod audio;
mod logger;
mod util;

use poise::{
    serenity_prelude::{CacheHttp, GatewayIntents, GuildId},
    Context as RawPoiseContext,
};

type Error = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = RawPoiseContext<'a, Data, Error>;

pub struct Data {
    pub audio_states: Mutex<HashMap<GuildId, Arc<AudioState>>>,
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, ctx, .. } => {
            if let Err(e) = send_embed(
                ctx.serenity_context().http(),
                ctx.channel_id(),
                &error.to_string(),
            )
            .await
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
            .filter_map(|g| Some(GuildId::new(g.parse().ok()?)))
            .collect();
    }
    vec![]
}

#[tokio::main]
async fn main() {
    logger::init_logger().expect("failed to init logger");
    let mut commands = vec![];
    audio::add_group(&mut commands);
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
    let framework = poise::Framework::builder()
        .options(options)
        .setup(|ctx, _, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    audio_states: Mutex::new(HashMap::new()),
                })
            })
        })
        .build();
    let token = env::var("OCTAVE_BOT_TOKEN").expect("Error: token not found");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS;
    let client = ClientBuilder::new(token, intents)
        .framework(framework)
        .register_songbird()
        .await;
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    client.unwrap().start().await.unwrap()
}
