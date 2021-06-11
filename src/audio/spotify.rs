use rspotify::{
    client::SpotifyBuilder,
    model::{
        Id,
        PlayableItem,
    },
    oauth2::CredentialsBuilder,
};

use super::{
    song::{
        Song,
        SongMetadata,
    },
    work::Work,
};

pub async fn get_playlist(playlist_id: &str) -> Result<Vec<(Song, Option<Work>)>, String>{
    let creds = CredentialsBuilder::default()
        .id("5f573c9620494bae87890c0f08a60293")
        .secret("212476d9b0f3472eaa762d90b19b0ba8")
        .build()
        .unwrap();
    let mut spotify = SpotifyBuilder::default()
        .credentials(creds)
        //.oauth(oauth)
        .build()
        .unwrap();
    if let Err(why) = spotify.request_client_token().await{
        println!("error: {}", why);
    };
    let playlist_id = Id::from_id(playlist_id);
    let playlist_id = match playlist_id{
        Ok(playlist_id) => playlist_id,
        Err(why) => {
            return Err(format!("spotify::get_playlist: {:?}", why));
        }
    };
    let tracks = spotify.playlist(playlist_id, None, None).await;
    let tracks = match tracks{
        Ok(tracks) => tracks,
        Err(why)=>{
            println!("Error in spotify.get_playlist: {:?}", why);
            return Err(format!("spotify::get_playlist: {:?}", why));
        }
    };
    let mut songs = Vec::new();
    let tracks = tracks.tracks.items;
    for data in tracks.iter() {
        let track = match &data.track{
            Some(PlayableItem::Track(track)) => track,
            Some(_) => continue,
            None => continue,
        };
        let artist = &track.artists[0].name;
        let title = &track.name;
        let metadata = SongMetadata{
            search_query: Some(get_query_string(artist, title)),
            artist: Some(artist.clone()),
            title: Some(title.clone()),
            youtube_url: None,
            duration: Some(track.duration.as_secs()),
        };
        match Song::new_load(metadata){
            Some(data) => songs.push(data),
            None => continue,
        };
    }
    Ok(songs)
}

fn get_query_string(artist: &str, title: &str) -> String{
    format!("{} {} lyrics", artist, title)
}