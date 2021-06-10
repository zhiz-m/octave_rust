
use tokio::process::Command;

pub async fn ytdl(query: &str) -> String{
    let mut cmd = Command::new("youtube-dl");
    let cmd = cmd
                       .arg("-x")
                       .arg("--skip-download")
                       .arg("--get-url")
                       .arg("--audio-quality").arg("128k")
                       .arg(format!("ytsearch:{}", query));
    let out = cmd.output().await.unwrap();
    println!("ytdl process finished");
    String::from_utf8(out.stdout).unwrap()
}
/*
fn ffmpeg(url: &str){
    let cmd = Command::new("ffmpeg")
                       .arg("-reconnect").arg("1")
                       .arg("-reconnect_streamed").arg("1")
                       .arg("-reconnect_delay_max").arg("5")
                       .arg("-i").arg(url)
                       .arg("-f").arg("s16le")
                       .arg("-ar").arg("48000")
                       .arg("-ac").arg("2")
                       .arg("pipe:1");
}*/