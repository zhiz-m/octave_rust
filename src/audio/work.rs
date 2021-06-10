use std::{
    sync::{Arc},
};
use tokio::sync::{mpsc, Mutex};

pub struct Work{
    pub sender: mpsc::Sender<String>,
    pub query: String,
    pub is_loaded: Arc<Mutex<bool>>,
}