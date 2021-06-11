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
    model::{channel::Message, id::GuildId}
};
use songbird::tracks::TrackCommand;
use tokio::{
    sync::Mutex,
};
use lazy_static::lazy_static;
use super::{
    audio_state::AudioState,
};

use crate::util::{
    send_message,
    send_embed,
    message_react,
};

#[group]
#[commands(join,disconnect,play,skip,pause,resume,change_loop,shuffle,clear,splay,queue)]
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
                    send_embed(ctx, msg, "Error: please be in a voice channel").await;
                    return None;
                }
            };
            let manager = songbird::get(ctx)
                .await
                .unwrap()
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

async fn remove_audio_state(ctx: &Context, msg: &Message) -> Result<(), String> {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let mut audio_states = AUDIO_STATES.lock().await;

    if let Some(state) = audio_states.remove(&guild_id){
        AudioState::cleanup(state).await;
        Ok(())
    }else{
        Err("bot is not currently active".to_string())
    }
}

#[command]
async fn join(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    if audio_state.is_some(){
        message_react(ctx, msg, "🥳").await;
    }

    Ok(())
}

#[command]
#[aliases("leave")]
async fn disconnect(ctx: &Context, msg: &Message) -> CommandResult{
    match remove_audio_state(ctx, msg).await{
        Ok(()) => message_react(ctx, msg, "👋").await,
        Err(why) => send_embed(ctx, msg, &format!("Error: {}", why)).await,
    };

    Ok(())
}

#[command]
async fn play(ctx: &Context, msg: &Message, args: Args) -> CommandResult{
    let query = args.rest();

    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    AudioState::add_audio(audio_state, query, false).await;

    message_react(ctx, msg, "🎶").await;

    Ok(())
}

#[command]
async fn splay(ctx: &Context, msg: &Message, args: Args) -> CommandResult{
    let query = args.rest();

    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    AudioState::add_audio(audio_state, query, true).await;

    message_react(ctx, msg, "🔀").await;
    message_react(ctx, msg, "🎶").await;

    Ok(())
}

#[command]
async fn skip(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    if let Err(why) = AudioState::send_track_command(audio_state, TrackCommand::Stop).await {
        send_embed(ctx, msg, &format!("Error: {}", why)).await;
    }else {
        message_react(ctx, msg, "↪").await;
    };
    Ok(())
}

#[command]
async fn pause(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    if let Err(why) = AudioState::send_track_command(audio_state, TrackCommand::Pause).await {
        send_embed(ctx, msg, &format!("Error: {}", why)).await;
    }else {
        message_react(ctx, msg, "⏸").await;
    };
    Ok(())
}

#[command]
async fn resume(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    if let Err(why) = AudioState::send_track_command(audio_state, TrackCommand::Play).await {
        send_embed(ctx, msg, &format!("Error: {}", why)).await;
    }else {
        message_react(ctx, msg, "▶").await;
    };
    Ok(())
}

#[command]
async fn shuffle(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    if let Err(why) = AudioState::shuffle(audio_state).await {
        send_embed(ctx, msg, &format!("Error: {}", why)).await;
    } else{
        message_react(ctx, msg, "🔀").await;
    };
    Ok(())
}

#[command]
async fn clear(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    if let Err(why) = AudioState::clear(audio_state.clone()).await {
        send_embed(ctx, msg, &format!("Error: {}", why)).await;
    } else{
        message_react(ctx, msg, "🗑").await;
    };

    Ok(())
}

#[command]
#[aliases("loop")]
async fn change_loop(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    match AudioState::change_looping(audio_state).await {
        Ok(true) => message_react(ctx, msg, "🔄").await,
        Ok(false) => message_react(ctx, msg, "➡").await,
        Err(why) => send_embed(ctx, msg, &format!("Error: {}", why)).await,
    };
    Ok(())
}

#[command]
async fn queue(ctx: &Context, msg: &Message) -> CommandResult{
    let audio_state = get_audio_state(ctx, msg).await;
    let audio_state = match audio_state{
        Some(audio_state) => audio_state,
        None => return Ok(())
    };

    send_embed(ctx, msg,&AudioState::get_string(audio_state).await).await;

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