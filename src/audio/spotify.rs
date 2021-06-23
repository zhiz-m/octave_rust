use rspotify::{
    client::{
        Spotify,
        SpotifyBuilder,
    },
    model::{
        Id,
        PlayableItem,
        Market,
        Country,
        FullTrack,
        SimplifiedTrack,
    },
    oauth2::CredentialsBuilder,
};

use rand::{
    seq::IteratorRandom,
    distributions::{
        WeightedIndex,
        Distribution,
    },
    Rng,
};

use tokio::{
    self,
    time::{
        sleep,
        Duration,
    },
};

use super::{
    song::{
        Song,
        SongMetadata,
    },
    work::Work,
    config::spotify_recommend as sr,
};

use std::sync::Arc;

pub enum TrackObject{
    FullTrack(FullTrack),
    SimplifiedTrack(SimplifiedTrack),
}

impl TrackObject{
    fn artist(&self) -> &str{
        match self{
            TrackObject::FullTrack(track) => &track.artists[0].name,
            TrackObject::SimplifiedTrack(track) => &track.artists[0].name,
        }
    }
    fn title(&self) -> &str{
        match self{
            TrackObject::FullTrack(track) => &track.name,
            TrackObject::SimplifiedTrack(track) => &track.name,
        }
    }
    fn duration(&self) -> u64{
        match self{
            TrackObject::FullTrack(track) => track.duration.as_secs(),
            TrackObject::SimplifiedTrack(track) => track.duration.as_secs(),
        }
    }
    fn album_id(&self) -> Option<&str>{
        match self{
            TrackObject::FullTrack(track) => track.album.id.as_deref(),
            TrackObject::SimplifiedTrack(_) => None,
        }
    }
    fn artist_id(&self) -> Option<&str>{
        match self{
            TrackObject::FullTrack(track) => track.artists[0].id.as_deref(),
            TrackObject::SimplifiedTrack(track) => track.artists[0].id.as_deref(),
        }
    }
}

pub struct SpotifyClient{
    client: Spotify,
}

impl SpotifyClient{
    pub async fn new() -> Result<SpotifyClient, String> {
        let creds = CredentialsBuilder::default()
            .id("5f573c9620494bae87890c0f08a60293")
            .secret("212476d9b0f3472eaa762d90b19b0ba8")
            .build();
        let creds = match creds{
            Ok(creds) => creds,
            Err(why) => return Err(why.to_string()),
        };
        let mut spotify = SpotifyBuilder::default()
            .credentials(creds)
            //.oauth(oauth)
            .build()
            .unwrap();
        if let Err(why) = spotify.request_client_token().await{
            return Err(why.to_string());
        };
        Ok(SpotifyClient{
            client: spotify,
        })
    }
    pub fn process_track_objects(tracks: Vec<TrackObject>) -> Vec<(Song, Option<Work>)> {
        let mut songs = vec![];
        for track in tracks.into_iter(){
            let artist = track.artist();
            let title = track.title();
            let metadata = SongMetadata{
                search_query: Some(SpotifyClient::get_query_string(artist, title)),
                artist: Some(artist.to_string()),
                title: Some(title.to_string()),
                youtube_url: None,
                duration: Some(track.duration()),
            };
            match Song::new_load(metadata){
                Some(data) => songs.push(data),
                None => continue,
            };
        }
        songs
    }
    pub async fn get_playlist(&self, playlist_id: &str) -> Result<Vec<TrackObject>, String>{
        let playlist_id = Id::from_id(playlist_id);
        let playlist_id = match playlist_id{
            Ok(playlist_id) => playlist_id,
            Err(why) => {
                return Err(format!("spotify::get_playlist: {:?}", why));
            }
        };
        let tracks = self.client.playlist(playlist_id, None, None).await;
        let tracks = match tracks{
            Ok(tracks) => tracks,
            Err(why)=>{
                println!("Error in spotify.get_playlist: {:?}", why);
                return Err(format!("spotify::get_playlist: {:?}", why));
            }
        };
        let items = tracks.tracks.items;
        let mut tracks = vec![];
        for data in items.into_iter() {
            let track = match data.track{
                Some(PlayableItem::Track(track)) => track,
                Some(_) => continue,
                None => continue,
            };
            tracks.push(TrackObject::FullTrack(track));
        }
        Ok(tracks)
    }
    async fn random_from_artist(&self, id: &str) -> Option<TrackObject>{
        let id = match Id::from_id(id){
            Ok(id) => id,
            Err(why) => {
                println!("Error {:?}",why);
                return None;
            }
        };
        let tracks = self.client.artist_top_tracks(id, &Market::Country(Country::Japan)).await;
        match tracks {
            Ok(tracks) => Some(TrackObject::FullTrack(tracks.into_iter().choose(&mut rand::thread_rng())?)),
            Err(why) => {
                println!("Error SpotifyClient::random_from_artist: {:?}", why);
                None
            },
        }
    }
    async fn random_from_album(&self, id: &str) -> Option<TrackObject>{
        let id = match Id::from_id(id){
            Ok(id) => id,
            Err(why) => {
                println!("Error {:?}",why);
                return None;
            }
        };
        let album = self.client.album(id).await;
        match album {
            Ok(album) => {
                let tracks = album.tracks.items;
                Some(TrackObject::SimplifiedTrack(tracks.into_iter().choose(&mut rand::thread_rng())?))
            },
            Err(why) => {
                println!("Error SpotifyClient::random_from_album: {:?}", why);
                None
            },
        }
    }
    //  -> Result<Vec<(Song, Option<Work>)>, String>
    pub async fn recommend_playlist(client: Arc<SpotifyClient>, amount: usize, playlist_id: &str) -> Result<Vec<TrackObject>, String>{
        let mut tasks = vec![];
        let tracks = match client.get_playlist(playlist_id).await{
            Ok(tracks) => tracks,
            Err(why) => return Err(why),
        };
        let tracks = Arc::new(tracks);
        //let tracks = tracks.sample(&mut rand::thread_rng(), amount);
        for _ in 0..amount{
            let tracks = tracks.clone();
            let ind = rand::thread_rng().gen::<u32>() as usize % tracks.len();
            
            let client = client.clone();
            let task = tokio::spawn(
                async move {
                    let track = &tracks[ind];

                    let weights = [sr::SAME_ARTIST, sr::EXPLORE_ALBUM, sr::EXPLORE_ARTIST];
                    let option = WeightedIndex::new(&weights).unwrap().sample(&mut rand::thread_rng());

                    match option{
                        // find random song from track artist
                        0 => {
                            let artist = match track.artist_id(){
                                Some(artist) => artist,
                                None => return None,
                            };
                            client.random_from_artist(artist).await
                        },
                        // find random song from track album
                        1 => {
                            let album = match track.album_id(){
                                Some(album) => album,
                                None => {
                                    println!("album not found");
                                    return None
                                },
                            };
                            client.random_from_album(album).await
                        },
                        // find random song from a random similar artist
                        _ => {
                            let artist = match track.artist_id(){
                                Some(artist) => artist,
                                None => return None,
                            };
                            let id = Id::from_id(artist).unwrap();
                            let artists = client.client.artist_related_artists(id).await;
                            let artists = match artists{
                                Ok(artists) => artists[..5].to_vec(),
                                Err(why) => {
                                    println!("Error related artists: {:?}", why);
                                    return None
                                }
                            };
                            let id = &artists.iter().choose(&mut rand::thread_rng()).unwrap().id;
                            client.random_from_artist(id).await
                        },
                    }
                }
            );
            tasks.push(task);
            sleep(Duration::from_millis(100)).await;
        };
        let mut tracks = vec![];
        for task in tasks.into_iter(){
            match task.await.unwrap(){
                Some(track) => tracks.push(track),
                None => continue,
            }
        }
        Ok(tracks)
    }
    fn get_query_string(artist: &str, title: &str) -> String{
        format!("{} {} lyrics", artist, title)
    }
}