use super::{
    spotify::get_playlist,
    song::{
        Song,
        SongMetadata,
    },
    work::Work,
};

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
        let songs = match get_playlist(playlist_id).await{
            Ok(songs) => songs,
            Err(why) => return Err(why),
        };
        return Ok(songs);
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