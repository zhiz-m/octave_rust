use tokio::{
    sync::{mpsc},
};
use super::{
    work::Work,
    subprocess::ytdl,
};

pub struct Loader {
    work: mpsc::Sender<Work>,
    kill: mpsc::Sender<()>,
}

impl Loader{
    pub async fn add_work(& self, work: Work){
        if let Err(err) = self.work.send(work).await{
            println!("Error in Loader::add_work: {}", err.to_string());
        };
    }
    async fn loader_loop(mut work: mpsc::Receiver<Work>,){
        while let Some(work) = work.recv().await {
            let url = ytdl(&work.query).await;
    
            if let Err(err) = work.sender.send(url).await{
                println!("Error in Loader::loader_loop: {:?}", err);
            };

            {
                let mut is_loaded = work.is_loaded.lock().await;
                assert!(!*is_loaded);
                *is_loaded = true;
            }
        }
    }

    pub async fn cleanup(&mut self) {
        if let Err(why) = self.kill.send(()).await{
            println!("Error on Loader::cleanup: {}",why);
        };
    }
    
    pub fn new() -> Loader{
        let (work_tx, work_sx) = mpsc::channel(200);
        let (kill_tx, kill_sx) = mpsc::channel(1);
        tokio::spawn(async move{
            Loader::start_loader_loop(work_sx, kill_sx).await
        });
        Loader{
            work: work_tx,
            kill: kill_tx,
        }
    }
    
    async fn start_loader_loop(work: mpsc::Receiver<Work>, mut kill: mpsc::Receiver<()>){
        let f = tokio::spawn(async move {
            Loader::loader_loop(work).await
        });
        kill.recv().await;
        f.abort();
    }
}