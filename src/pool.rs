use std::{collections::BTreeMap, sync::{atomic::AtomicUsize, mpsc::{self}}};

use log::info;
use thiserror::Error;

use crate::{message::Message, task::Task, worker::{self, Worker}};

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Worker {0} not found")]
    WorkerNotFound(usize),

    #[error("Woker error: {0}")]
    WorkerError(#[from] worker::WorkerError),
}

pub struct ThreadPool {
    n_threads: usize,
    workers: BTreeMap<usize, Worker>,
    _private: (),
}

impl ThreadPool {
    pub (crate) fn new(n_threads: usize) -> Self {
        assert!(n_threads > 0);
        Self { n_threads, workers: BTreeMap::new(), _private: () }
    }

    pub (crate) fn spawn(&mut self, task: Task) -> Result<(), PoolError> {
        info!("Spawning task with ID {}", task.id);
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match self.workers.get(&(&id % self.n_threads)) {
            Some(worker) => worker.send_message(Message::NewTask(task)),
            None => return Err(PoolError::WorkerNotFound(id % self.n_threads))
        };

        Ok(())
    }

    pub (crate) fn start(&mut self) -> Result<(), PoolError> {
        info!("Starting {} workers", self.n_threads);
        for i in 0..self.n_threads {
            let (tx, rx) = mpsc::channel::<Message>();
            let mut worker = Worker::new(i, tx);
            worker.start(rx)?;
            self.workers.insert(i, worker);
        }
        Ok(())
    }

    pub (crate) fn kill(&mut self) {
        info!("Killing all workers");
        for (_, worker) in self.workers.iter_mut() {
            worker.send_message(Message::Kill);
        }
    }

    pub (crate) fn shutdown(&mut self) -> Result<(), PoolError> {
        info!("Shutting down all workers");
        for (_, worker) in self.workers.iter_mut() {
            worker.send_message(Message::Shutdown);
        }
        self.join()?;
        Ok(())
    }

    pub (crate) fn join(&mut self) -> Result<(), PoolError> {
        info!("Joining all workers");
        for (_, worker) in self.workers.iter_mut() {
            worker.join()?;
        }
        Ok(())
    }

    pub (crate) fn num_threads(&self) -> usize {
        self.n_threads
    }
            
}