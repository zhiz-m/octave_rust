use serenity::{
    client::Context, 
    model::{
        channel::Message, 
        prelude::ReactionType,
        id::ChannelId,
    },
    http::Http,
};

use std::sync::Arc;

pub async fn send_message(ctx: &Context, msg: &Message, text: &str){
    let res = msg.channel_id.send_message(&ctx.http, |m| {
        m.content(text);
        m
    }).await;
    if let Err(why) = res{
        println!("Error sending embed: {:?}", why);
    };
}

pub async fn send_embed(ctx: &Context, msg: &Message, text: &str){
    let res = msg.channel_id.send_message(&ctx.http, |m| {
        m.embed(|e| {
            e.colour(0xf542bf);
            e.description(text);
            e
        });
        m
    }).await;
    if let Err(why) = res{
        println!("Error sending embed: {:?}", why);
    };
}

pub async fn send_embed_http(channel_id: ChannelId, http: Arc<Http>, text: &str){
    let res = channel_id.send_message(http, |m| {
        m.embed(|e| {
            e.colour(0xf542bf);
            e.description(text);
            e
        });
        m
    }).await;

    if let Err(why) = res{
        println!("Error sending embed: {:?}", why);
    };
}

pub async fn message_react(ctx: &Context, msg: &Message, emoji: &str){
    let res = msg.react(&ctx.http, ReactionType::Unicode(emoji.to_string())).await;
    if let Err(why) = res{
        println!("Error reacting to message: {:?}", why);
    }
}