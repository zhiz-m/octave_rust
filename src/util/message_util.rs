use poise::serenity_prelude::{ChannelId, CreateEmbed, Http};

pub fn get_styled_embed<'a>(e: &'a mut CreateEmbed, text: &str) -> &'a mut CreateEmbed {
    e.colour(0xf542bf);
    e.description(text);
    e
}

pub async fn send_embed(http: &Http, channel_id: ChannelId, text: &str) -> anyhow::Result<()> {
    channel_id
        .send_message(http, |m| {
            m.embed(|e| get_styled_embed(e, text));
            m
        })
        .await?;
    Ok(())
}
/*pub async fn send_message(ctx: &Context, channel_id: ChannelId, text: &str) -> anyhow::Result<()> {
    channel_id
        .send_message(&ctx.http, |m| {
            m.content(text);
            m
        })
        .await?;
    Ok(())
}
pub async fn message_react(ctx: &Context, msg: &Message, emoji: &str) {
    let res = msg
        .react(&ctx.http, ReactionType::Unicode(emoji.to_string()))
        .await;
    if let Err(why) = res {
        println!("Error reacting to message: {:?}", why);
    }
}
*/
