use std::{
    io::{BufReader, Read},
    process::{
        Command,
        Stdio,
    },
};
use tokio::process::Command as TokioCommand;

pub async fn ytdl(query: &str) -> String{
    let mut cmd = TokioCommand::new("youtube-dl");
    let cmd = cmd
        .arg("-x")
        .arg("--skip-download")
        .arg("--get-url")
        .arg("--audio-quality").arg("128k")
        .arg(query);
    let out = cmd.output().await.unwrap();
    String::from_utf8(out.stdout).unwrap()
}
//  -> Result<Box<dyn Read + Send>, String>
pub async fn ffmpeg_pcm(url: String) -> Result<Box<dyn Read + Send>, String>{
    /*let res = tokio::task::spawn_blocking(move ||{
        
    }).await.unwrap();
    res*/
    let mut cmd = Command::new("ffmpeg");
    let cmd = cmd
        .arg("-reconnect").arg("1")
        .arg("-reconnect_streamed").arg("1")
        .arg("-reconnect_delay_max").arg("5")
        .arg("-i").arg(url)
        .arg("-f").arg("s16le")
        .arg("-ar").arg("48000")
        .arg("-ac").arg("2")
        .arg("-acodec").arg("pcm_f32le")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = match cmd.spawn(){
        Ok(child) => child,
        Err(error) => {
            return Err(format!("{}",error));
        }
    };
    let out = match child.stdout{
        Some(out) => out,
        None => return Err("subprocess::ffmpeg_pcm: failed to get child stdout".to_string()),
    };
    let buf = BufReader::with_capacity(16384*8, out);
    let buf: Box<dyn Read + Send> = Box::new(buf);
    Ok(buf)
}


/*
fn ffmpeg(url: &str){
    
}*/