use poise::serenity_prelude::{ChannelId, CreateEmbed, Http};
use serenity::all::CreateMessage;

pub fn get_styled_embed(text: &str) -> CreateEmbed {
    CreateEmbed::new().colour(0xf542bf).description(text)
}

pub async fn send_embed(http: &Http, channel_id: ChannelId, text: &str) -> anyhow::Result<()> {
    channel_id
        .send_message(http, CreateMessage::new().add_embed(get_styled_embed(text)))
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
