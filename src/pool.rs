use std::{os::fd::AsRawFd, sync::{atomic::AtomicUsize, Arc}};
use dashmap::DashMap;
use log::info;
use rand::seq::SliceRandom;
use thiserror::Error;

use crate::{event_callback_registry, event_poller, task::Task, worker::{self, WorkStealer, Worker}};

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Worker {0} not found")]
    WorkerNotFound(usize),

    #[error("Woker error: {0}")]
    WorkerError(#[from] worker::WorkerError),

    #[error("Event poller error: {0}")]
    EventPollerError(#[from] event_poller::EventPollerError),

    #[error("Event callback registry error: {0}")]
    EventCallbackRegistryError(#[from] event_callback_registry::EventCallbackRegistryError),

    #[error("Failed to join worker thread")]
    JoinError
}

pub struct ThreadPool {
    n_threads: usize,
    workers: DashMap<usize, Worker>,
    stealers: DashMap<usize, Arc<Vec<WorkStealer>>>,
}

impl ThreadPool {
    pub (crate) fn new(n_threads: usize) -> Self {
        assert!(n_threads > 0);
        Self { n_threads, workers: DashMap::new(), stealers: DashMap::new() }
    }

    fn get_next_worker(
        &self
    ) -> Result<dashmap::mapref::one::RefMut<'_, usize, Worker>, PoolError> {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let next_worker_id = id % self.n_threads;

        self.workers.get_mut(
            &next_worker_id
        ).ok_or(
            PoolError::WorkerNotFound(next_worker_id)
        )
    }

    pub fn spawn(&self, task: Task) -> Result<(), PoolError> {
        info!("Spawning task with ID {}", task.id);
        Ok(self.get_next_worker()?.spawn(task)?)
    }

    pub fn register_interest<T, H>(
        &self,
        source: &T, 
        handler: H,
        interest_type: event_poller::InterestType,
    ) -> Result<(), PoolError> 
    where 
        T: AsRawFd,
        H: event_callback_registry::EventHandler + Send + Sync + 'static
    {
        let worker = self.get_next_worker()?;
        let event_registry = worker.event_registry();
        let callback_registry = worker.callback_registry();

        let key = event_registry.register_interest(source, interest_type)?;
        callback_registry.register(key, handler)?;

        Ok(())
    }

    pub fn start(&self) -> Result<(), PoolError> {
        info!("Starting {} workers", self.n_threads);
        for i in 0..self.n_threads {
            let mut worker = Worker::new(i)?;
            worker.start()?;
            self.workers.insert(i, worker);
        }
        Ok(())
    }

    pub fn kill(&self) -> Result<(), PoolError> {
        info!("Killing all workers");
        Ok(for i in 0..self.n_threads {
            let mut worker = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(i))?;
            worker.kill()?;
        })
    }

    pub fn shutdown(&self) -> Result<(), PoolError> {
        info!("Shutting down all workers");
        for i in 0..self.n_threads {
            let mut worker = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(i))?;
            worker.shutdown()?;
        }
        self.join()
    }

    pub fn join(&self) -> Result<(), PoolError> {
        info!("Joining all workers");
        Ok(for i in 0..self.n_threads {
            let mut worker_guard = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(i))?;
            let handle = worker_guard.take_join_handle();
            drop(worker_guard);
            if let Some(handle) = handle {
                handle.join().map_err(|_| PoolError::JoinError)?;
            }
        })
    }

    pub fn num_threads(&self) -> usize {
        self.n_threads
    }

    pub fn get_stealers_rand(&self, worker_id: usize) -> Arc<Vec<WorkStealer>> {
        Arc::clone(&self.stealers.entry(worker_id).or_insert({
            let mut stealers = Vec::with_capacity(self.n_threads);
            for multi in self.workers.iter() {
                if *multi.key() == worker_id { continue; }
                stealers.push(multi.value().stealer());
            }
            stealers.shuffle(&mut rand::rng());
            Arc::new(stealers)
        }))
    }
            
}