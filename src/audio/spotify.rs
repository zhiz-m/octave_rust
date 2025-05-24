use anyhow::Context;
use rspotify::{
    clients::BaseClient,
    model::{
        AlbumId, ArtistId, Country, FullTrack, Market, PlayableItem, PlaylistId, SimplifiedTrack,
        TrackId,
    },
    ClientCredsSpotify, Credentials,
};

use rand::{
    distributions::{Distribution, WeightedIndex},
    seq::{IteratorRandom, SliceRandom},
    Rng,
};

use tokio::{
    self,
    time::{sleep, Duration},
};

use super::{
    config::{self, spotify_recommend as sr},
    song::{self, Song, SongMetadata},
    types::StreamType,
};

use std::{env, sync::Arc};

pub enum TrackObject {
    FullTrack(FullTrack),
    SimplifiedTrack(SimplifiedTrack),
}

impl TrackObject {
    fn artist(&self) -> &str {
        match self {
            TrackObject::FullTrack(track) => &track.artists[0].name,
            TrackObject::SimplifiedTrack(track) => &track.artists[0].name,
        }
    }
    fn title(&self) -> &str {
        match self {
            TrackObject::FullTrack(track) => &track.name,
            TrackObject::SimplifiedTrack(track) => &track.name,
        }
    }
    fn duration(&self) -> i64 {
        match self {
            TrackObject::FullTrack(track) => track.duration.num_seconds(),
            TrackObject::SimplifiedTrack(track) => track.duration.num_seconds(),
        }
    }
    fn album_id(&self) -> Option<&AlbumId> {
        match self {
            TrackObject::FullTrack(track) => track.album.id.as_ref(),
            TrackObject::SimplifiedTrack(_) => None,
        }
    }
    fn artist_id(&self) -> Option<&ArtistId> {
        match self {
            TrackObject::FullTrack(track) => track.artists[0].id.as_ref(),
            TrackObject::SimplifiedTrack(track) => track.artists[0].id.as_ref(),
        }
    }
}

pub struct SpotifyClient {
    client: ClientCredsSpotify,
}

impl SpotifyClient {
    pub async fn new() -> anyhow::Result<SpotifyClient> {
        // old credentials, delete soon
        // let creds = Credentials::new(
        //     "5f573c9620494bae87890c0f08a60293",
        //     "212476d9b0f3472eaa762d90b19b0ba8",
        // );
        let creds = Credentials::new(
            &env::var(config::env::SPOTIFY_CLIENT_ID).expect("Error: token not found"),
            &env::var(config::env::SPOTIFY_CLIENT_SECRET).expect("Error: token not found"),
        );
        let spotify = ClientCredsSpotify::new(creds);
        spotify.request_token().await?;
        Ok(SpotifyClient { client: spotify })
    }

    pub fn process_track_objects(tracks: Vec<TrackObject>, stream_type: StreamType) -> Vec<Song> {
        tracks
            .iter()
            .map(|track| {
                let artist = track.artist();
                let title = track.title();
                let how_to_find =
                    song::HowToFind::SearchQuery(SpotifyClient::get_query_string(artist, title));
                let metadata = SongMetadata {
                    artist: Some(artist.to_string()),
                    title: Some(title.to_string()),
                    duration: Some(track.duration() as u64),
                    how_to_find,
                };

                Song::new_load(metadata, stream_type)
            })
            .collect()
    }

    pub async fn get_playlist(
        &self,
        playlist_id: PlaylistId<'_>,
    ) -> anyhow::Result<Vec<TrackObject>> {
        let tracks = self.client.playlist(playlist_id, None, None).await?;
        let items = tracks.tracks.items;
        let tracks = items
            .into_iter()
            .filter_map(|data| match data.track {
                Some(PlayableItem::Track(track)) => Some(TrackObject::FullTrack(track)),
                Some(_) => None,
                None => None,
            })
            .collect();
        Ok(tracks)
    }
    pub async fn get_album(&self, album_id: AlbumId<'_>) -> anyhow::Result<Vec<TrackObject>> {
        let tracks = self.client.album(album_id, None).await?;
        let items = tracks.tracks.items;
        let tracks = items
            .into_iter()
            .map(TrackObject::SimplifiedTrack)
            .collect();
        Ok(tracks)
    }
    pub async fn get_track(&self, track_id: TrackId<'_>) -> anyhow::Result<TrackObject> {
        let track = self.client.track(track_id, None).await?;
        Ok(TrackObject::FullTrack(track))
    }
    async fn random_from_artist(&self, id: ArtistId<'_>) -> anyhow::Result<TrackObject> {
        let tracks = self
            .client
            .artist_top_tracks(id, Some(Market::Country(Country::Japan)))
            .await?;
        Ok(TrackObject::FullTrack(
            tracks
                .into_iter()
                .choose(&mut rand::thread_rng())
                .context("returned tracks was empty")?,
        ))
    }
    async fn random_from_album(&self, id: AlbumId<'_>) -> anyhow::Result<TrackObject> {
        let album = self.client.album(id, None).await?;
        Ok(TrackObject::SimplifiedTrack(
            album
                .tracks
                .items
                .into_iter()
                .choose(&mut rand::thread_rng())
                .context("returned tracks was empty")?,
        ))
    }
    //  -> Result<Vec<(Song, Option<Work>)>, String>
    pub async fn recommend_playlist(
        self: Arc<Self>,
        amount: usize,
        playlist_id: PlaylistId<'_>,
    ) -> anyhow::Result<Vec<TrackObject>> {
        let mut tasks = vec![];
        let tracks = self.get_playlist(playlist_id).await?;
        let tracks = Arc::new(tracks);
        //let tracks = tracks.sample(&mut rand::thread_rng(), amount);
        for _ in 0..amount {
            let tracks = tracks.clone();
            let ind = rand::thread_rng().gen::<u32>() as usize % tracks.len();

            let client = self.clone();
            let task = tokio::spawn(async move {
                let track = &tracks[ind];

                let weights = [sr::SAME_ARTIST, sr::EXPLORE_ALBUM, sr::EXPLORE_ARTIST];
                let option = WeightedIndex::new(weights)
                    .unwrap()
                    .sample(&mut rand::thread_rng());

                match option {
                    // find random song from track artist
                    0 => {
                        let artist = track.artist_id().context("failed to find artist")?;
                        client.random_from_artist(artist.clone()).await
                    }
                    // find random song from track album
                    1 => {
                        let album = track.album_id().context("album not found")?;
                        client.random_from_album(album.clone()).await
                    }
                    // find random song from a random similar artist
                    _ => {
                        Err(anyhow::anyhow!("spotdl: unsupported"))
                        // let artist = track.artist_id().context("failed to find artist")?;
                        // let artists = client.client.artist_related_artists(artist).await?;
                        // let artists = artists[..5].to_vec();
                        // let id = &artists.iter().choose(&mut rand::thread_rng()).unwrap().id;
                        // client.random_from_artist(id).await
                    }
                }
            });
            tasks.push(task);
            sleep(Duration::from_millis(100)).await;
        }
        let mut tracks = vec![];
        for task in tasks.into_iter() {
            tracks.push(task.await??)
        }
        tracks.shuffle(&mut rand::thread_rng());
        Ok(tracks)
    }
    fn get_query_string(artist: &str, title: &str) -> String {
        format!("{} {} lyrics", artist, title)
    }
}
