use std::{collections::VecDeque, sync::Arc};

use super::{
    subprocess::get_pcm_reader_config,
    types::{QueuePosition, Work},
};
use anyhow::{Context, Ok};
use tokio::sync::{Mutex, Semaphore};

pub struct SongLoader {
    work: Arc<Mutex<VecDeque<Work>>>,
    work_sem: Arc<Semaphore>,
}

impl SongLoader {
    pub async fn add_work(&self, work: Work, queue_position: QueuePosition) -> anyhow::Result<()> {
        {
            let mut queue = self.work.lock().await;
            match queue_position {
                QueuePosition::Front => queue.push_front(work),
                QueuePosition::Back => queue.push_back(work),
            }
        }
        self.work_sem.add_permits(1);
        Ok(())
    }
    async fn loader_loop(
        work: Arc<Mutex<VecDeque<Work>>>,
        work_sem: Arc<Semaphore>,
    ) -> anyhow::Result<()> {
        while let Some(work) = {
            {
                match work_sem.is_closed() {
                    true => None,
                    false => {
                        work_sem.acquire().await.unwrap().forget();
                        let work = work.clone().lock().await.pop_front().unwrap();
                        Some(work)
                    }
                }
            }
        } {
            // todo: error shouldnt exit this loop
            let buf_config = get_pcm_reader_config(&work.query, work.stream_type).await?;

            work.sender
                .send(Some(buf_config))
                .await
                .context("failed to send work")?;

            {
                let mut is_loaded = work.is_loaded.lock().await;
                assert!(!*is_loaded);
                *is_loaded = true;
            }
        }
        Ok(())
    }

    pub async fn cleanup(&mut self) -> anyhow::Result<()> {
        self.work_sem.close();
        Ok(())
    }

    pub fn new() -> Self {
        let work = Arc::new(Mutex::new(VecDeque::new()));
        let work_sem = Arc::new(Semaphore::new(0));
        tokio::spawn({
            let work = work.clone();
            let work_sem = work_sem.clone();
            async move { Self::loader_loop(work, work_sem).await }
        });
        Self { work, work_sem }
    }
}
