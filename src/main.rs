use serenity::{async_trait, client::{Client, Context, EventHandler}, framework::{
        StandardFramework,
    }, model::gateway::Ready};

use songbird::{
    SerenityInit,
};

mod audio;

struct Handler;

#[async_trait]
impl EventHandler for Handler{
    async fn ready(&self, _: Context, _: Ready){
        println!("bot connected");
    }
}


#[tokio::main]
async fn main() {
    let token = "ODQyMzUyNjcxMTE2NDkyODIy.YJ0EDg.JP9PZHKGc23ZF_5W-j-gipnbYW8";
    let framework = StandardFramework::new()
        .configure(|c|{
            c.prefix("a.")
        })
        .group(&audio::AUDIO_GROUP);
    
    let mut client = Client::builder(token)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await{
        println!("Error starting client: {:?}", why);
    }
}