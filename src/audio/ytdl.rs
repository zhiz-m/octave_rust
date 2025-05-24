use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::process::Command as TokioCommand;

use super::{
    song::{self, Song, SongMetadata},
    types::StreamType,
};

pub async fn ytdl_get_source_url(query: &str) -> String {
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

#[derive(Debug, Serialize, Deserialize)]
struct PlaylistTrackInfo {
    // Core fields (always present)
    id: String,
    title: String,
    url: String,
    uploader: Option<String>,
    duration: Option<f64>, // Seconds (float for partial seconds)
}

pub async fn ytdl_process_playlist(
    playlist_url: &str,
    stream_type: StreamType,
) -> anyhow::Result<Vec<Song>> {
    let mut cmd = TokioCommand::new("yt-dlp");
    let cmd = cmd
        .arg("-x")
        .arg("--flat-playlist")
        .arg("-j")
        .arg(playlist_url);
    let out = cmd.output().await.unwrap();
    let stdout = String::from_utf8(out.stdout)?;
    let songs = stdout
        .split('\n')
        .filter_map(|line| {
            match serde_json::from_str::<PlaylistTrackInfo>(line)
                .context("failed to parse output from ytdl playlist query")
            {
                Err(err) => {
                    log::error!("ytdl_process_playlist error: {err} {line}");
                    None
                }
                Ok(track_info) => {
                    let metadata = SongMetadata {
                        artist: track_info.uploader,
                        title: Some(track_info.title),
                        how_to_find: song::HowToFind::YoutubeTrackUrl(track_info.url),
                        duration: track_info.duration.map(|duration| duration as u64),
                    };
                    Some(Song::new_load(metadata, stream_type))
                }
            }
        })
        .collect();
    Ok(songs)
}
