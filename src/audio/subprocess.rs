use std::{io::{
        BufReader, 
        Read, 
        Write,
    }, mem::drop, process::{
        Command,
        Stdio,
        ChildStdin,
    }, str, sync::Arc, time::{Duration, Instant}};
use tokio::{io::AsyncWriteExt, process::Command as TokioCommand};
use super::work::StreamType;

#[derive(Clone)]
struct LoudnormConfig{
    integrated: f64,
    true_peak: f64,
    lra: f64,
    threshold: f64,
}

#[derive(Clone)]
pub struct PcmReaderConfig{
    buf: Option<Vec<u8>>,
    volume_delta: Option<f64>,
    stream_type: StreamType,
    src_url: String
}

pub async fn get_pcm_reader_config(youtube_url: &str, stream_type: StreamType) -> Result<PcmReaderConfig, String>{
    let src_url = ytdl(youtube_url).await;
    match stream_type {
        StreamType::Online => Ok(PcmReaderConfig{
            buf: None,
            volume_delta: None,
            stream_type, 
            src_url
        }),
        StreamType::Loudnorm => {
            let buf = download_audio_buf(src_url.clone()).await?;
            let loudnorm = get_loudnorm_params(&buf).await?;
            let buf = ffmpeg_loudnorm_convert(buf, loudnorm).await?;
            let volume_delta = ffmpeg_get_volume(&buf).await?;
            Ok(PcmReaderConfig{
                buf: Some(buf),
                volume_delta: Some(volume_delta),
                stream_type, 
                src_url
            })
        }
    }
    
}

async fn ytdl(query: &str) -> String{
    let mut cmd = TokioCommand::new("youtube-dl");
    let cmd = cmd
        .arg("-x")
        .arg("--skip-download")
        .arg("--get-url")
        //.arg("--audio-quality").arg("128k")
        .arg(query);
    let out = cmd.output().await.unwrap();
    String::from_utf8(out.stdout).unwrap()
}
/*
async fn download_audio(mut url: String) -> Result<Vec<u8>, String> {
    url.pop();
    println!("url: {}", url);
    let now = Instant::now();
    let mut cmd = TokioCommand::new("curl");
    let cmd = cmd
        .arg(url)
        .stderr(Stdio::null());
    let res = cmd.output().await;
    println!("audio downloaded, time: {:?}", now.elapsed());
    match res{
        Ok(out) => {
            println!("stderr: {}", str::from_utf8(&out.stderr).unwrap());
            Ok(out.stdout)
        },
        Err(why) => Err(why.to_string()),
    }
}
*/
async fn download_audio_buf(url:String) -> Result<Vec<u8>, String> {
    let now = Instant::now();
    let mut cmd = TokioCommand::new("ffmpeg");
    let out = cmd
        .arg("-reconnect").arg("1")
        .arg("-reconnect_streamed").arg("1")
        .arg("-reconnect_delay_max").arg("5")
        .arg("-i").arg(url)
        .arg("-f").arg("mp3")
        .arg("pipe:1")
        .output().await.unwrap();
    println!("audio downloaded, time: {:?}", now.elapsed());
    Ok(out.stdout)
}

async fn get_loudnorm_params(buf: &[u8]) -> Result<LoudnormConfig, String> {
    let mut cmd = TokioCommand::new("ffmpeg");
    let cmd = cmd
        .arg("-f").arg("mp3")
        .arg("-i").arg("pipe:0")
        .arg("-af").arg("loudnorm=I=-16:LRA=11:TP=-1.5:print_format=summary")
        .arg("-vn").arg("-sn").arg("-dn")
        .arg("-f").arg("mp3")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn(){
        Ok(child) => child,
        Err(why) => return Err(why.to_string()),
    };
    let stdin = child.stdin.as_mut().unwrap();
    if let Err(x) = stdin.write_all(buf).await{
        println!("Warning: subprocess::get_loudnorm_params error: {}", x.to_string());
    };
    drop(stdin);

    let out = match child.wait_with_output().await{
        Ok(out) => out,
        Err(why) => return Err(why.to_string()),
    };
    return Ok(parse_loudnorm_params( str::from_utf8(&out.stderr).unwrap())?)
}

fn parse_loudnorm_params(buf: &str) -> Result<LoudnormConfig, String> {
    let res = LoudnormConfig{
        integrated: _parse_loudnorm_params(buf, "Input Integrated:")?,
        true_peak: _parse_loudnorm_params(buf, "Input True Peak:")?,
        lra: _parse_loudnorm_params(buf, "Input LRA:")?,
        threshold: _parse_loudnorm_params(buf, "Input Threshold:")?,
    };
    return Ok(res);
}

fn _parse_loudnorm_params(buf: &str, target: &str) -> Result<f64, String> {
    let split = match buf.split(target).nth(1){
        Some(split) => split,
        None => return Err(format!("subprocess:: _parse_loudnorm_params failed to find substring {}", target)),
    };
    
    match split.split(" ").filter(|&x| x.len()>0).nth(0){
        Some(res) => Ok(res.parse::<f64>().unwrap()),
        None => return Err(format!("subprocess:: _parse_loudnorm_params failed to find item value for {}", target)),
    }
} 

async fn ffmpeg_get_volume(buf: &[u8]) -> Result<f64, String> {
    let mut cmd = TokioCommand::new("ffmpeg");
    let cmd = cmd
        .arg("-f").arg("mp3")
        .arg("-i").arg("pipe:0")
        .arg("-af").arg("volumedetect")
        .arg("-vn").arg("-sn").arg("-dn")
        .arg("-f").arg("mp3")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn(){
        Ok(child) => child,
        Err(why) => return Err(why.to_string()),
    };
    let stdin = child.stdin.as_mut().unwrap();
    /*if let Err(why) = stdin.write_all(buf){
        return Err(why.to_string());
    };*/
    if let Err(why) = stdin.write_all(buf).await {
        println!("warning: subprocess::ffmpeg_get_volume error: {}", why.to_string());
    };
    drop(stdin);

    let out = match child.wait_with_output().await{
        Ok(out) => out,
        Err(why) => return Err(why.to_string()),
    };

    let out = match str::from_utf8(&out.stderr){
        Ok(out) => out,
        Err(why) => return Err(why.to_string()),
    };

    //println!("output: {}", out);

    let split = match out.split("max_volume: ").nth(1){
        Some(split) => split,
        None => return Err("ffmpeg volumedetect output not recognised".to_string()),
    };
    
    match split.split(" ").nth(0){
        Some(vol) => Ok(-(vol.parse::<f64>().unwrap())),
        None => return Err("ffmpeg volumedetect output failed to parse as integer".to_string()),
    }
}

fn pipe_stdin(buf: &[u8], mut pipe: ChildStdin) {
    if let Err(x) = pipe.write_all(buf){
        println!("Warning: subprocess::pipe_stdin error: {}", x.to_string());
    };
}

async fn ffmpeg_loudnorm_convert(buf: Vec<u8>, loudnorm: LoudnormConfig) -> Result<Vec<u8>, String> {
    let loudnorm_string = format!("loudnorm=I=-16:LRA=11:TP=-1.5:measured_I={:.2}:measured_LRA={:.2}:measured_TP={:.2}:measured_thresh={:.2}", 
        loudnorm.integrated, loudnorm.lra, loudnorm.true_peak, loudnorm.threshold);
    let mut cmd = TokioCommand::new("ffmpeg");
    let cmd = cmd
        .arg("-f").arg("mp3")
        .arg("-i").arg("pipe:0")
        .arg("-af").arg(loudnorm_string)
        .arg("-vn").arg("-sn").arg("-dn")
        .arg("-f").arg("mp3")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = match cmd.spawn(){
        Ok(child) => child,
        Err(why) => return Err(why.to_string()),
    };
    let stdin = child.stdin.as_mut().unwrap();
    if let Err(x) = stdin.write_all(&buf).await{
        println!("Warning: subprocess::get_loudnorm_params error: {}", x.to_string());
    };
    drop(stdin);

    let out = match child.wait_with_output().await{
        Ok(out) => out,
        Err(why) => return Err(why.to_string()),
    };
    return Ok(out.stdout)
}

fn ffmpeg_pcm_loudnorm(buf: Vec<u8>, loudnorm: LoudnormConfig) -> Result<Box<dyn Read + Send>, String>{
    let loudnorm_string = format!("loudnorm=I=-16:LRA=11:TP=-1.5:measured_I={:.2}:measured_LRA={:.2}:measured_TP={:.2}:measured_thresh={:.2}", 
                                        loudnorm.integrated, loudnorm.lra, loudnorm.true_peak, loudnorm.threshold);
    let mut cmd = Command::new("ffmpeg");
    let cmd = cmd
        .arg("-f").arg("mp3")
        .arg("-i").arg("pipe:0")
        .arg("-af").arg(loudnorm_string)
        .arg("-f").arg("s16le")
        .arg("-ar").arg("48000")
        .arg("-ac").arg("2")
        .arg("-acodec").arg("pcm_f32le")
        .arg("pipe:1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = match cmd.spawn(){
        Ok(child) => child,
        Err(error) => {
            return Err(format!("{}",error));
        }
    };
    let stdin = match child.stdin{
        Some(stdin) => stdin,
        None => return Err("subprocess::ffmpeg_pcm: failed to get child stdin".to_string()),
    };

    tokio::task::spawn_blocking(move || {
        pipe_stdin(&buf[..], stdin);
    });

    let stdout = match child.stdout{
        Some(stdout) => stdout,
        None => return Err("subprocess::ffmpeg_pcm: failed to get child stdout".to_string()),
    };
    let buf = BufReader::with_capacity(16384*32, stdout);
    let buf: Box<dyn Read + Send> = Box::new(buf);
    Ok(buf)
}

// for loudnorm, requires existing, downloaded buffer
pub async fn get_pcm_reader(config: PcmReaderConfig) -> Result<Box<dyn Read + Send>, String>{
    let mut cmd = Command::new("ffmpeg");
    //println!("get_pcm_reader src_url: ");
    let cmd = match config.stream_type {
        StreamType::Online => {
            cmd
            .arg("-reconnect").arg("1")
            .arg("-reconnect_streamed").arg("1")
            .arg("-reconnect_delay_max").arg("5")
            .arg("-i").arg(config.src_url.clone())
            .arg("-f").arg("s16le")
            //.arg("-af").arg("loudnorm")
            .arg("-ar").arg("48000")
            .arg("-ac").arg("2")
            .arg("-acodec").arg("pcm_f32le")
            .arg("pipe:1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
        }
        StreamType::Loudnorm => {
            cmd
            .arg("-f").arg("mp3")
            .arg("-i").arg("pipe:0")
            .arg("-af").arg(format!("volume={:.0}dB", config.volume_delta.unwrap()))
            .arg("-f").arg("s16le")
            .arg("-ar").arg("48000")
            .arg("-ac").arg("2")
            .arg("-acodec").arg("pcm_f32le")
            .arg("pipe:1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
        }
    };
    
    let child = match cmd.spawn(){
        Ok(child) => child,
        Err(error) => {
            return Err(format!("{}",error));
        }
    };

    if let StreamType::Loudnorm = config.stream_type{
        let stdin = match child.stdin{
            Some(stdin) => stdin,
            None => return Err("subprocess::ffmpeg_pcm: failed to get child stdin".to_string()),
        };
    
        tokio::task::spawn_blocking(move || {
            pipe_stdin(config.buf.unwrap().as_ref(), stdin);
        });
    }

    let stdout = match child.stdout{
        Some(stdout) => stdout,
        None => return Err("subprocess::ffmpeg_pcm: failed to get child stdout".to_string()),
    };
    let buf = BufReader::with_capacity(16384*32, stdout);
    let buf: Box<dyn Read + Send> = Box::new(buf);
    Ok(buf)
}

// streams live from youtube
pub fn ffmpeg_pcm(url: String) -> Result<Box<dyn Read + Send>, String>{
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
        .arg("-af").arg("loudnorm")
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
    let buf = BufReader::with_capacity(16384*32, out);
    let buf: Box<dyn Read + Send> = Box::new(buf);
    Ok(buf)
}


/*
fn ffmpeg(url: &str){
    
}*/