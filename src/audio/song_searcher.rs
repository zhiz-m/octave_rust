use super::{
    spotify::SpotifyClient,
    song::{
        Song,
        SongMetadata,
    },
    work::Work,
};

use std::sync::Arc;

pub async fn process_query(query: &str) -> Result<Vec<(Song, Option<Work>)>, String>{
    if query.contains("spotify") && query.contains("/playlist/"){
        let split: Vec<&str> = query
            .split("/playlist/")
            .filter(|s| !s.is_empty())
            .collect();
        if split.len() != 2 {
            return Err("invalid spotify playlist URL".to_string());
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
        let tracks = match client.get_playlist(playlist_id).await{
            Ok(tracks) => tracks,
            Err(why) => return Err(why),
        };
        return Ok(SpotifyClient::process_track_objects(tracks));
        /*
        
        */
    } else {
        let data = if query.contains("watch?v=") {
            (Some(query.to_string()), None)
        } else {
            (None, Some(query.to_string()))
        };
        let metadata = SongMetadata{
            artist: None,
            title: None,
            duration: None,
            search_query: data.1,
            youtube_url: data.0,
        };
        let song = match Song::new_load(metadata){
            Some(song) => song,
            None => return Err("failed to get song from YouTube".to_string()),
        };
        return Ok(vec![song]);
    };
}

pub async fn song_recommender(query: &str, amount: usize) -> Result<Vec<(Song, Option<Work>)>, String>{
    let split: Vec<&str> = query
        .split("/playlist/")
        .filter(|s| !s.is_empty())
        .collect();
    if split.len() != 2 {
        return Err("invalid spotify playlist URL".to_string());
    }
    let playlist_id = split[1];
    let playlist_id = playlist_id
        .split('?')
        .find(|s| !s.is_empty())
        .expect("Logical error: process_query's playlist_id contains items?");
    
    let client = SpotifyClient::new().await;
    let client = match client {
        Ok(client) => Arc::new(client),
        Err(why) => return Err(why),
    };
    let tracks = match SpotifyClient::recommend_playlist(client, amount, playlist_id).await{
        Ok(tracks) => tracks,
        Err(why) => return Err(why),
    };
    return Ok(SpotifyClient::process_track_objects(tracks));
}