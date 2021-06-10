use std::{
    sync::Arc,
    collections::HashMap,
};
use serenity::{
    client::Context, 
    framework::standard::{
        Args, CommandResult,
        macros::{command, group},
    },
    http::Http, model::{channel::Message, id::{ChannelId, GuildId}}};
use tokio::{
    sync::Mutex,
};
use lazy_static::lazy_static;
use super::{
    audio_state::AudioState,
};

#[group]
#[commands(play)]
struct Audio;

lazy_static! {
    static ref AUDIO_STATES: Mutex<HashMap<GuildId, Arc<AudioState>>> = {
        Mutex::new(HashMap::new())
    };
}

async fn get_audio_state(ctx: &Context, msg: &Message) -> Option<Arc<AudioState>> {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let mut audio_states = AUDIO_STATES.lock().await;

    match audio_states.get(&guild_id) {
        Some(state) => {
            Some(state.clone())
        }
        None => {
            let channel_id = guild
                .voice_states
                .get(&msg.author.id)
                .and_then(|voice_state| voice_state.channel_id);
            let channel_id = match channel_id{
                Some(channel_id) => channel_id,
                None => {
                    return None;
                }
            };
            let manager = songbird::get(ctx)
                .await
                .expect("Err get_audio_state: Songbird not initialized")
                .clone();
            let (handle_lock, success) = manager.join(guild_id, channel_id).await;
            if let Err(err) = success{
                println!("Error: {:?}", err);
                return None;
            }
            let audio_state = AudioState::new(handle_lock);
            {
                audio_states.insert(guild_id, audio_state.clone());
            }
            Some(audio_state)
        }
    }
}

#[command]
async fn play(ctx: &Context, msg: &Message, args: Args) -> CommandResult{
    let query = args.rest();
    println!("{}",query);

    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
        
    };

    AudioState::add_audio(audio_state, query).await;

    Ok(())
}
/*
#[command]
async fn start(ctx: &Context, msg: &Message) -> CommandResult{
    let c_id = msg.channel_id;
    let b = ctx.clone();
    println!("start command");
    //let a= a.clone();
    {
        let b = b.clone();
        tokio::spawn(async move {
                spam(c_id, b, 3).await
            }
        );
    }
    {
        let b = b.clone();
        tokio::spawn(async move {
                spam(c_id, b, 5).await
            }
        );
    }
    Ok(())
}

async fn spam(c_id: ChannelId, ctx: Context, duration: u64){
    loop {
        let result = c_id.say(&ctx.http, format!("{} seconds passed", duration)).await;
        if let Err(why) = result {
            println!("Error sending message {:?}", why);
        }
        sleep(Duration::from_secs(duration)).await;
    }
}  
*/