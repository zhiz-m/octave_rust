use super::{subprocess::get_pcm_reader_config, work::Work};
use anyhow::anyhow;
use tokio::sync::mpsc;

pub struct YoutubeLoader {
    work: mpsc::Sender<Work>,
    kill: mpsc::Sender<()>,
}

impl YoutubeLoader {
    pub async fn add_work(&self, work: Work) -> anyhow::Result<()> {
        self.work.send(work).await.map_err(|e|anyhow!(e.to_string()))?;
        Ok(())
    }
    async fn loader_loop(mut work: mpsc::Receiver<Work>) -> anyhow::Result<()> {
        while let Some(work) = work.recv().await {
            let buf_config = get_pcm_reader_config(&work.query, work.stream_type).await.map_err(|e|anyhow!(e.to_string()))?;

            work.sender.send(Some(buf_config)).await.map_err(|e|anyhow!(e.to_string()))?;

            {
                let mut is_loaded = work.is_loaded.lock().await;
                assert!(!*is_loaded);
                *is_loaded = true;
            }
        }
        Ok(())
    }

    pub async fn cleanup(&mut self) -> anyhow::Result<()> {
        self.kill.send(()).await.map_err(|e|anyhow!(e.to_string()))?;
        Ok(())
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
