use std::sync::Arc;

use anyhow::anyhow;

use super::{
    song::{Song, SongMetadata},
    spotify::SpotifyClient,
    work::{StreamType, Work},
};

pub async fn process_query(
    query: &str,
    stream_type: StreamType,
) -> anyhow::Result<Vec<(Song, Option<Work>)>> {
    if query.contains("spotify") && query.contains("/playlist/") {
        let split: Vec<&str> = query
            .split("/playlist/")
            .filter(|s| !s.is_empty())
            .collect();
        if split.len() != 2 {
            return Err(anyhow!("invalid spotify playlist URL"));
        }
        let playlist_id = split[1];
        let playlist_id = playlist_id
            .split('?')
            .find(|s| !s.is_empty())
            .expect("Logical error: process_query's playlist_id contains items?");

        let client = SpotifyClient::new().await;
        let client = match client {
            Ok(client) => client,
            Err(why) => return Err(why),
        };
        let tracks = match client.get_playlist(playlist_id).await {
            Ok(tracks) => tracks,
            Err(why) => return Err(why),
        };
        Ok(SpotifyClient::process_track_objects(tracks, stream_type))
    } else if query.contains("spotify") && query.contains("/track/") {
        let split: Vec<&str> = query.split("/track/").filter(|s| !s.is_empty()).collect();
        if split.len() != 2 {
            return Err(anyhow!("invalid spotify track URL"));
        }
        let playlist_id = split[1];
        let playlist_id = playlist_id
            .split('?')
            .find(|s| !s.is_empty())
            .expect("Logical error: process_query's track_id contains items?");

        let client = SpotifyClient::new().await;
        let client = match client {
            Ok(client) => client,
            Err(why) => return Err(why),
        };
        let track = match client.get_track(playlist_id).await {
            Ok(track) => track,
            Err(why) => return Err(why),
        };
        Ok(SpotifyClient::process_track_objects(
            vec![track],
            stream_type,
        ))
    } else {
        let data = if query.contains("watch?v=") {
            (Some(query.to_string()), None)
        } else {
            (None, Some(query.to_string()))
        };
        let metadata = SongMetadata {
            artist: None,
            title: None,
            duration: None,
            search_query: data.1,
            youtube_url: data.0,
        };
        let song = match Song::new_load(metadata, stream_type) {
            Some(song) => song,
            None => return Err(anyhow!("failed to get song from YouTube")),
        };
        Ok(vec![song])
    }
}

pub async fn song_recommender(
    query: &str,
    amount: usize,
    stream_type: StreamType,
) -> anyhow::Result<Vec<(Song, Option<Work>)>> {
    let split: Vec<&str> = query
        .split("/playlist/")
        .filter(|s| !s.is_empty())
        .collect();
    if split.len() != 2 {
        return Err(anyhow!("invalid spotify playlist URL"));
    }
    let playlist_id = split[1];
    let playlist_id = playlist_id
        .split('?')
        .find(|s| !s.is_empty())
        .expect("Logical error: process_query's playlist_id contains items?");

    let client = Arc::new(SpotifyClient::new().await?);
    let tracks = client.recommend_playlist(amount, playlist_id).await?;
    Ok(SpotifyClient::process_track_objects(tracks, stream_type))
}
