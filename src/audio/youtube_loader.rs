use super::{subprocess::get_pcm_reader_config, work::Work};
use tokio::sync::mpsc;

pub struct YoutubeLoader {
    work: mpsc::Sender<Work>,
    kill: mpsc::Sender<()>,
}

impl YoutubeLoader {
    pub async fn add_work(&self, work: Work) {
        if let Err(err) = self.work.send(work).await {
            println!("Error in Loader::add_work: {}", err);
        };
    }
    async fn loader_loop(mut work: mpsc::Receiver<Work>) {
        while let Some(work) = work.recv().await {
            let buf_config = match get_pcm_reader_config(&work.query, work.stream_type).await {
                Ok(buf) => Some(buf),
                Err(why) => {
                    println!("Error in Loader::loader_loop: {:?}", why);
                    None
                }
            };

            if let Err(why) = work.sender.send(buf_config).await {
                println!("Error in Loader::loader_loop: {}", why);
            };

            {
                let mut is_loaded = work.is_loaded.lock().await;
                assert!(!*is_loaded);
                *is_loaded = true;
            }
        }
    }

    pub async fn cleanup(&mut self) {
        if let Err(why) = self.kill.send(()).await {
            println!("Error on Loader::cleanup: {}", why);
        };
    }

    pub fn new() -> YoutubeLoader {
        let (work_tx, work_sx) = mpsc::channel(200);
        let (kill_tx, kill_sx) = mpsc::channel(1);
        tokio::spawn(async move { YoutubeLoader::start_loader_loop(work_sx, kill_sx).await });
        YoutubeLoader {
            work: work_tx,
            kill: kill_tx,
        }
    }

    async fn start_loader_loop(work: mpsc::Receiver<Work>, mut kill: mpsc::Receiver<()>) {
        let f = tokio::spawn(async move { YoutubeLoader::loader_loop(work).await });
        kill.recv().await;
        f.abort();
    }
}
