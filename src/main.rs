use audio::config;
use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::StandardFramework,
    model::gateway::Ready,
    prelude::GatewayIntents,
};
use std::env;

use songbird::SerenityInit;

mod audio;
mod util;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, _: Ready) {
        println!("bot connected");
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("OCTAVE_BOT_TOKEN").expect("Error: token not found");
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
        println!("Error starting client: {:?}", why);
    }
}
