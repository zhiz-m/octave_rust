use audio::{config, audio_state::AudioState};
use util::send_embed;
use std::{env, collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

mod audio;
mod util;
mod logger;

use poise::{Context as RawPoiseContext, serenity_prelude::{GuildId, GatewayIntents}};

type Error = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = RawPoiseContext<'a, Data, Error>;

pub struct Data{
    audio_states: Mutex<HashMap<GuildId, Arc<AudioState>>>,
}

impl Data{
    pub fn new() -> Data{
        Data { audio_states: Mutex::new(HashMap::new()) }
    }

    pub fn get_audio_states(&self) -> &Mutex<HashMap<GuildId, Arc<AudioState>>>{
        &self.audio_states
    }
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, ctx } => {
            if let Err(e) = send_embed(ctx.discord(), ctx.channel_id(), &error.to_string()).await{
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

#[tokio::main]
async fn main() {
    logger::init_logger().expect("failed to init logger");
    let mut commands = vec![];
    audio::add_group(&mut commands);
    let options = poise::FrameworkOptions{
        commands,
        on_error: |error| Box::pin(on_error(error)),
        ..Default::default()
    };
    poise::Framework::builder()
        .token(env::var("OCTAVE_BOT_TOKEN").expect("Error: token not found"))
        .options(options)
        .intents(GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::GUILD_VOICE_STATES
            | GatewayIntents::MESSAGE_CONTENT
            | GatewayIntents::GUILDS)
        .user_data_setup(|_,_,_| Box::pin(async move {
            Ok(Data::new())
        }))
        .run()
        .await
        .expect("client error");
    /*let token = env::var("OCTAVE_BOT_TOKEN").expect("Error: token not found");
    let framework = StandardFramework::new()
        .configure(|c| {
            //c.prefix("a.")
            c.prefix(config::audio::BOT_PREFIX)
        })
        .group(&audio::AUDIO_GROUP);

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        info!("Error starting client: {:?}", why);
    }*/
}
