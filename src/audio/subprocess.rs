use crate::audio::config;

use super::types::{AudioReaderConfig, StreamType};
use anyhow::{anyhow, Context};
use songbird::input::core::io::MediaSource;
use std::{
    io::{BufReader, Cursor},
    process::{Command, Stdio},
    str,
    time::Instant,
};
use symphonia::core::io::ReadOnlySource;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command as TokioCommand,
    time::timeout,
};

#[derive(Clone)]
struct LoudnormConfig {
    integrated: f64,
    true_peak: f64,
    lra: f64,
    threshold: f64,
}

// pub struct AudioReaderConfig {
//     buf: Option<Vec<u8>>,
//     // volume_delta: Option<f64>,
//     stream_type: StreamType,
//     src_url: String,
// }

// pub struct BufReaderSeek<T: Read + Send> {
//     inner: BufReader<T>,
// }

// impl<T: Read + Send> BufReaderSeek<T> {
//     pub fn new(inner: BufReader<T>) -> BufReaderSeek<T> {
//         BufReaderSeek { inner }
//     }
// }

// impl<T: Read + Send + Sync> MediaSource for BufReaderSeek<T> {
//     fn is_seekable(&self) -> bool {
//         false
//     }

//     fn byte_len(&self) -> Option<u64> {
//         None
//     }
// }

// impl<T: Read + Send> Seek for BufReaderSeek<T> {
//     fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
//         Err(std::io::Error::new(
//             std::io::ErrorKind::Other,
//             "seek not supported",
//         ))
//     }
// }

// impl<T: Read + Send> Read for BufReaderSeek<T> {
//     fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
//         let res = self.inner.read(buf);
//         println!("read {:?} bytes", res);
//         res
//     }
// }

pub async fn get_audio_reader_config(
    ytdl_query: &str,
    stream_type: StreamType,
) -> anyhow::Result<AudioReaderConfig> {
    let src_url = timeout(config::audio::YTDL_QUERY_RETRY_INTERVAL, ytdl(ytdl_query)).await?;
    match stream_type {
        StreamType::Online => Ok(AudioReaderConfig::Online { src_url }),
        StreamType::Loudnorm => {
            let buf = timeout(
                config::audio::YTDL_DOWNLOAD_RETRY_INTERVAL,
                download_audio_buf(src_url.clone()),
            )
            .await??;
            let loudnorm = get_loudnorm_params(buf.clone()).await?;
            let buf = ffmpeg_loudnorm_convert(buf, loudnorm).await?;
            // let volume_delta = ffmpeg_get_volume(&buf).await?;
            Ok(AudioReaderConfig::Loudnorm { buf })
        }
    }
}

async fn ytdl(query: &str) -> String {
    let mut cmd = TokioCommand::new("yt-dlp");
    let cmd = cmd
        .arg("-x")
        .arg("--skip-download")
        .arg("--get-url")
        //.arg("--audio-quality").arg("128k")
        .arg(query);
    let out = cmd.output().await.unwrap();

    // let error = String::from_utf8(out.stderr).unwrap();
    // println!("youtube-dl returned {}, err {}", &result, &error);
    String::from_utf8(out.stdout).unwrap()
}
/*
async fn download_audio(mut url: String) -> Result<Vec<u8>, String> {
    url.pop();
    info!("url: {}", url);
    let now = Instant::now();
    let mut cmd = TokioCommand::new("curl");
    let cmd = cmd
        .arg(url)
        .stderr(Stdio::null());
    let res = cmd.output().await;
    info!("audio downloaded, time: {:?}", now.elapsed());
    match res{
        Ok(out) => {
            info!("stderr: {}", str::from_utf8(&out.stderr).unwrap());
            Ok(out.stdout)
        },
        Err(why) => Err(why.to_string()),
    }
}
*/

// static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

//todo: do this in-process using HLS
async fn download_audio_buf(url: String) -> anyhow::Result<Vec<u8>> {
    let now = Instant::now();
    let mut cmd = TokioCommand::new("ffmpeg");
    let out = cmd
        .arg("-reconnect")
        .arg("1")
        .arg("-reconnect_streamed")
        .arg("1")
        .arg("-reconnect_delay_max")
        .arg("5")
        .arg("-i")
        .arg(url)
        .arg("-f")
        .arg("mp3")
        .arg("pipe:1")
        .output()
        .await
        .unwrap();
    log::info!("audio downloaded, time: {:?}", now.elapsed());
    Ok(out.stdout)
}

async fn get_loudnorm_params(buf: Vec<u8>) -> anyhow::Result<LoudnormConfig> {
    let mut cmd = TokioCommand::new("ffmpeg");
    let cmd = cmd
        .arg("-i")
        .arg("pipe:0")
        .arg("-af")
        .arg("loudnorm=I=-16:LRA=11:TP=-1.5:print_format=summary")
        .arg("-vn")
        .arg("-sn")
        .arg("-dn")
        .arg("-f")
        .arg("mp3")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let child = cmd.spawn().context("failed to spawn child")?;
    let mut stdin = child.stdin.unwrap();
    {
        let future = async move {
            if let Err(x) = stdin
                .write_all(&buf)
                .await
                .context("failed to write data to buffer")
            {
                log::warn!("Warning: subprocess::get_loudnorm_params error: {}", x);
            };
            if let Err(x) = stdin.shutdown().await.context("failed to shutdown stdin") {
                log::warn!("Warning: subprocess::get_loudnorm_params error: {}", x);
            };
        };
        tokio::task::spawn(future);
    }

    let mut buf = vec![];
    child
        .stderr
        .unwrap()
        .read_to_end(&mut buf)
        .await
        .context("failed to wait for child stderr")?;
    parse_loudnorm_params(str::from_utf8(&buf).unwrap())
}

fn parse_loudnorm_params(buf: &str) -> anyhow::Result<LoudnormConfig> {
    let res = LoudnormConfig {
        integrated: _parse_loudnorm_params(buf, "Input Integrated:")?,
        true_peak: _parse_loudnorm_params(buf, "Input True Peak:")?,
        lra: _parse_loudnorm_params(buf, "Input LRA:")?,
        threshold: _parse_loudnorm_params(buf, "Input Threshold:")?,
    };
    Ok(res)
}

fn _parse_loudnorm_params(buf: &str, target: &str) -> anyhow::Result<f64> {
    let split = match buf.split(target).nth(1) {
        Some(split) => split,
        None => {
            return Err(anyhow::anyhow!(
                "subprocess:: _parse_loudnorm_params failed to find substring {}",
                target
            ))
        }
    };

    match split.split(' ').find(|&x| !x.is_empty()) {
        Some(res) => Ok(res.parse::<f64>().unwrap()),
        None => Err(anyhow::anyhow!(
            "subprocess:: _parse_loudnorm_params failed to find item value for {}",
            target
        )),
    }
}

// async fn ffmpeg_get_volume(buf: &[u8]) -> anyhow::Result<f64> {
//     let mut cmd = TokioCommand::new("ffmpeg");
//     let cmd = cmd
//         .arg("-f")
//         .arg("mp3")
//         .arg("-i")
//         .arg("pipe:0")
//         .arg("-af")
//         .arg("volumedetect")
//         .arg("-vn")
//         .arg("-sn")
//         .arg("-dn")
//         .arg("-f")
//         .arg("mp3")
//         .arg("-")
//         .stdin(Stdio::piped())
//         .stdout(Stdio::null())
//         .stderr(Stdio::piped());
//     let mut child = cmd.spawn().context("failed to spawn child")?;
//     let stdin = child.stdin.as_mut().unwrap();

//     if let Err(why) = stdin.write_all(buf).await {
//         log::warn!("warning: subprocess::ffmpeg_get_volume error: {}", why);
//     };

//     let out = child
//         .wait_with_output()
//         .await
//         .context("failed to wait for child output")?;
//     let out = str::from_utf8(&out.stderr).context("failed to parse str from utf8")?;
//     let split = out
//         .split("max_volume: ")
//         .nth(1)
//         .context("ffmpeg volumedetect output not recognised")?;
//     match split.split(' ').next() {
//         Some(vol) => Ok(-(vol.parse::<f64>().unwrap())),
//         None => Err(anyhow::anyhow!(
//             "ffmpeg volumedetect output failed to parse as integer".to_string()
//         )),
//     }
// }

// async fn pipe_stdin(buf: &[u8], mut pipe: ChildStdin) {
//     if let Err(x) = pipe.write_all(buf).await {
//         log::warn!("Warning: subprocess::pipe_stdin error: {}", x);
//     };
// }

async fn ffmpeg_loudnorm_convert(
    buf: Vec<u8>,
    loudnorm: LoudnormConfig,
) -> anyhow::Result<Vec<u8>> {
    let loudnorm_string = format!("loudnorm=I=-16:LRA=11:TP=-1.5:measured_I={:.2}:measured_LRA={:.2}:measured_TP={:.2}:measured_thresh={:.2}", 
        loudnorm.integrated, loudnorm.lra, loudnorm.true_peak, loudnorm.threshold);
    let mut cmd = TokioCommand::new("ffmpeg");
    let cmd = cmd
        .arg("-i")
        .arg("pipe:0")
        .arg("-af")
        .arg(loudnorm_string)
        .arg("-vn")
        .arg("-sn")
        .arg("-dn")
        .arg("-ar")
        .arg("48000")
        .arg("-ac")
        .arg("2")
        .arg("-f")
        .arg("mp3")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let child = cmd.spawn().context("failed to spawn child")?;
    let mut stdin = child.stdin.unwrap();
    {
        let future = async move {
            if let Err(x) = stdin
                .write_all(&buf)
                .await
                .context("failed to write data to buffer")
            {
                log::warn!("Warning: subprocess::get_loudnorm_params error: {}", x);
            };
            if let Err(x) = stdin.shutdown().await.context("failed to shutdown stdin") {
                log::warn!("Warning: subprocess::get_loudnorm_params error: {}", x);
            };
        };
        tokio::task::spawn(future);
    }

    let mut buf = vec![];
    child
        .stdout
        .unwrap()
        .read_to_end(&mut buf)
        .await
        .context("failed to fetch output from child")?;
    Ok(buf)
}

/*fn ffmpeg_pcm_loudnorm(
    buf: Vec<u8>,
    loudnorm: LoudnormConfig,
) -> Result<Box<dyn Read + Send>, String> {
    let loudnorm_string = format!("loudnorm=I=-16:LRA=11:TP=-1.5:measured_I={:.2}:measured_LRA={:.2}:measured_TP={:.2}:measured_thresh={:.2}",
                                        loudnorm.integrated, loudnorm.lra, loudnorm.true_peak, loudnorm.threshold);
    let mut cmd = Command::new("ffmpeg");
    let cmd = cmd
        .arg("-f")
        .arg("mp3")
        .arg("-i")
        .arg("pipe:0")
        .arg("-af")
        .arg(loudnorm_string)
        .arg("-f")
        .arg("s16le")
        .arg("-ar")
        .arg("48000")
        .arg("-ac")
        .arg("2")
        .arg("-acodec")
        .arg("pcm_f32le")
        .arg("pipe:1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(format!("{}", error));
        }
    };
    let stdin = match child.stdin {
        Some(stdin) => stdin,
        None => return Err("subprocess::ffmpeg_pcm: failed to get child stdin".to_string()),
    };

    tokio::task::spawn_blocking(move || {
        pipe_stdin(&buf[..], stdin);
    });

    let stdout = match child.stdout {
        Some(stdout) => stdout,
        None => return Err("subprocess::ffmpeg_pcm: failed to get child stdout".to_string()),
    };
    let buf = BufReader::with_capacity(16384 * 32, stdout);
    let buf: Box<dyn Read + Send> = Box::new(buf);
    Ok(buf)
}*/

// for loudnorm, requires existing, downloaded buffer
pub async fn get_audio_reader(
    config: AudioReaderConfig,
) -> anyhow::Result<Box<dyn MediaSource + Send>> {
    let mut cmd = Command::new("ffmpeg");
    match config {
        AudioReaderConfig::Online { src_url } => {
            // songbird supports synchronous IO only, or a synchronous wrapper around async IO,
            // hence we're not using TokioCommand
            let cmd = cmd
                .arg("-reconnect")
                .arg("1")
                .arg("-reconnect_streamed")
                .arg("1")
                .arg("-reconnect_delay_max")
                .arg("5")
                .arg("-i")
                .arg(src_url)
                .arg("-f")
                // .arg("s16le")
                .arg("mp3")
                //.arg("-af").arg("loudnorm")
                .arg("-ar")
                .arg("48000")
                .arg("-ac")
                .arg("2")
                // .arg("-acodec")
                // .arg("pcm_f32le")
                .arg("pipe:1")
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let child = cmd.spawn().context("failed to spawn child")?;

            let stdout = child
                .stdout
                .context("subprocess::get_audio_reader: failed to get child stdout")?;
            let buf = BufReader::with_capacity(16384 * 32 * 32, stdout);
            Ok(Box::new(ReadOnlySource::new(buf)))
        }
        //todo: consider whether one-pass loudnorm is enough. that way we can cut-through stream audio for loudnorm instead of downloading all at once.
        AudioReaderConfig::Loudnorm { buf } => {
            // cmd
            //     .arg("-f")
            //     .arg("mp3")
            //     .arg("-i")
            //     .arg("pipe:0")
            //     .arg("-af")
            //     .arg(format!("volume={:.0}dB", config.volume_delta.unwrap()))
            //     .arg("-f")
            //     // .arg("s16le")
            //     .arg("mp3")
            //     .arg("-ar")
            //     .arg("48000")
            //     .arg("-ac")
            //     .arg("2")
            //     // .arg("-acodec")
            //     // .arg("pcm_f32le")
            //     .arg("pipe:1")
            //     .stdin(Stdio::piped())
            //     .stdout(Stdio::piped())
            //     .stderr(Stdio::null()),
            let reader = Cursor::new(buf);
            Ok(Box::new(ReadOnlySource::new(reader)))
        }
        AudioReaderConfig::Error => Err(anyhow!("error loading audio, skipping")),
    }
}

/*
// streams live from youtube
pub fn ffmpeg_pcm(url: String) -> Result<Box<dyn MediaSource + Send>, String> {
    /*let res = tokio::task::spawn_blocking(move ||{

    }).await.unwrap();
    res*/
    let mut cmd = Command::new("ffmpeg");
    let cmd = cmd
        .arg("-reconnect")
        .arg("1")
        .arg("-reconnect_streamed")
        .arg("1")
        .arg("-reconnect_delay_max")
        .arg("5")
        .arg("-i")
        .arg(url)
        .arg("-f")
        .arg("s16le")
        .arg("-af")
        .arg("loudnorm")
        .arg("-ar")
        .arg("48000")
        .arg("-ac")
        .arg("2")
        .arg("-acodec")
        .arg("pcm_f32le")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(format!("{}", error));
        }
    };
    let out = match child.stdout {
        Some(out) => out,
        None => return Err("subprocess::ffmpeg_pcm: failed to get child stdout".to_string()),
    };
    let buf = BufReader::with_capacity(16384 * 32, out);
    let buf = Box::new(BufReaderSeek::<ChildStdout>::new(buf));
    Ok(buf)
}
*/
/*
fn ffmpeg(url: &str){

}*/
