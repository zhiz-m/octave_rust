use rspotify::model as rspotify;
use std::sync::Arc;

use anyhow::anyhow;

use super::{
    song::{HowToFind, Song, SongMetadata},
    spotify::SpotifyClient,
    types::StreamType,
    ytdl,
};

#[derive(Clone)]
enum Query<'a> {
    SpotifyPlaylist(rspotify::PlaylistId<'a>),
    SpotifyAlbum(rspotify::AlbumId<'a>),
    SpotifyTrack(rspotify::TrackId<'a>),
    YoutubeTrack(HowToFind),
    YoutubePlaylist { url: String },
}

fn parse_query<'a>(query: &'a str) -> anyhow::Result<Query<'a>> {
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
        let playlist_id = rspotify::PlaylistId::from_id(playlist_id)?;
        Ok(Query::SpotifyPlaylist(playlist_id))
    } else if query.contains("spotify") && query.contains("/track/") {
        let split: Vec<&str> = query.split("/track/").filter(|s| !s.is_empty()).collect();
        if split.len() != 2 {
            return Err(anyhow!("invalid spotify track URL"));
        }
        let track_id = split[1];
        let track_id = track_id
            .split('?')
            .find(|s| !s.is_empty())
            .expect("Logical error: process_query's track_id contains items?");
        let track_id = rspotify::TrackId::from_id(track_id)?;
        Ok(Query::SpotifyTrack(track_id))
    } else if query.contains("spotify") && query.contains("/album/") {
        let split: Vec<&str> = query.split("/album/").filter(|s| !s.is_empty()).collect();
        if split.len() != 2 {
            return Err(anyhow!("invalid spotify album URL"));
        }
        let album_id = split[1];
        let album_id = album_id
            .split('?')
            .find(|s| !s.is_empty())
            .expect("Logical error: process_query's album_id contains items?");
        let album_id = rspotify::AlbumId::from_id(album_id)?;
        Ok(Query::SpotifyAlbum(album_id))
    } else if query.contains("youtube") && query.contains("playlist") {
        Ok(Query::YoutubePlaylist {
            url: query.to_string(),
        })
    } else if query.contains("youtube") {
        let how_to_find = if query.contains("watch?v=") {
            HowToFind::YoutubeTrackUrl(query.to_string())
        } else {
            HowToFind::SearchQuery(query.to_string())
        };
        Ok(Query::YoutubeTrack(how_to_find))
    } else {
        Err(anyhow!("unrecongized query {query}"))
    }
}

pub async fn process_query(query: &str, stream_type: StreamType) -> anyhow::Result<Vec<Song>> {
    let query = parse_query(query)?;
    match query {
        Query::SpotifyPlaylist(playlist_id) => {
            let client = SpotifyClient::new().await?;
            let tracks = client.get_playlist(playlist_id).await?;
            Ok(SpotifyClient::process_track_objects(tracks, stream_type))
        }
        Query::SpotifyTrack(track_id) => {
            let client = SpotifyClient::new().await?;
            let track = client.get_track(track_id).await?;
            Ok(SpotifyClient::process_track_objects(
                vec![track],
                stream_type,
            ))
        }
        Query::SpotifyAlbum(album_id) => {
            let client = SpotifyClient::new().await?;
            let tracks = client.get_album(album_id).await?;
            Ok(SpotifyClient::process_track_objects(tracks, stream_type))
        }
        Query::YoutubeTrack(how_to_find) => {
            let metadata = SongMetadata {
                artist: None,
                title: None,
                duration: None,
                how_to_find,
            };
            let song = Song::new_load(metadata, stream_type);
            Ok(vec![song])
        }
        Query::YoutubePlaylist { url } => ytdl::ytdl_process_playlist(&url, stream_type).await,
    }
}

pub async fn song_recommender(
    query: &str,
    amount: usize,
    stream_type: StreamType,
) -> anyhow::Result<Vec<Song>> {
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
    let playlist_id = rspotify::PlaylistId::from_id(playlist_id)?;
    let tracks = client.recommend_playlist(amount, playlist_id).await?;
    Ok(SpotifyClient::process_track_objects(tracks, stream_type))
}
