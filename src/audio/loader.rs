use tokio::{
    sync::{mpsc},
};
use super::{
    work::Work,
    youtube::ytdl,
};

pub struct Loader {
    work: mpsc::Sender<Work>,
    kill: mpsc::Sender<()>,
}

impl Loader{
    pub async fn kill(& self){
        if let Err(err) = self.kill.send(()).await{
            println!("Error in Loader.kill: {}", err.to_string());
        };
    }
    pub async fn add_work(& self, work: Work){
        if let Err(err) = self.work.send(work).await{
            println!("Error in Loader.kill: {}", err.to_string());
        };
    }
    async fn loader_loop(mut work: mpsc::Receiver<Work>,){
        loop{
            let work = match work.recv().await{
                Some(work)=>work,
                None=>break,
            };
            println!("hi2");
            // update this
            let url = ytdl(&work.query).await;
    
            if let Err(err) = work.sender.send(url).await{
                println!("Error in loader_loop: {:?}", err);
            };

            {
                let mut is_loaded = work.is_loaded.lock().await;
                assert!(*is_loaded == false);
                *is_loaded = true;
            }
        }
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

