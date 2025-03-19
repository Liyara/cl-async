use std::{
    os::fd::AsRawFd, 
    sync::atomic::AtomicUsize
};
use dashmap::DashMap;
use thiserror::Error;

use crate::{
    events::{
        handler::registry::EventHandlerRegistryError, 
        poller::{registry::EventPollerRegistryError, EventPollerError, InterestType}, EventHandler
    }, io::IOOperation, worker::{WorkerError, WorkerIOSubmissionHandle, WorkerState}, Task, Worker
};

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Worker {0} not found")]
    WorkerNotFound(usize),

    #[error("Woker error: {0}")]
    WorkerError(#[from] WorkerError),

    #[error("Event Poller error: {0}")]
    EventPollerError(#[from] EventPollerError),

    #[error("Event Handler Registry error: {0}")]
    EventCallbackRegistryError(#[from] EventHandlerRegistryError),

    #[error("Event Poller Registry error: {0}")]
    EventPollerRegistryError(#[from] EventPollerRegistryError),

    #[error("No workers available")]
    NoWorkersAvailable,

    #[error("Failed to join worker thread")]
    JoinError
}

pub struct ThreadPool {
    n_threads: usize,
    workers: DashMap<usize, Worker>,
}

impl ThreadPool {
    pub (crate) fn new(n_threads: usize) -> Self {
        assert!(n_threads > 0);
        Self { n_threads, workers: DashMap::new() }
    }

    fn next_worker_id(&self) -> usize {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        id % self.n_threads
    }

    fn get_next_worker(
        &self
    ) -> Result<dashmap::mapref::one::RefMut<'_, usize, Worker>, PoolError> {
        for _ in 0..self.n_threads {
            
            let next_worker_id = self.next_worker_id();

            let worker = self.workers.get_mut(
                &next_worker_id
            ).ok_or(
                PoolError::WorkerNotFound(next_worker_id)
            )?;

            let state = worker.get_state();

            if 
                state == WorkerState::None ||
                state == WorkerState::Stopped ||
                state == WorkerState::Stopping
            { continue }

            return Ok(worker);
        }
        Err(PoolError::NoWorkersAvailable)
    }

    pub fn spawn(&self, task: Task) -> Result<(), PoolError> {
        log::info!("Spawning task with ID {}", task.id);
        Ok(self.get_next_worker()?.spawn(task)?)
    }

    pub fn spawn_multiple(&self, tasks: Vec<Task>) -> Result<(), PoolError> {
        Ok(self.get_next_worker()?.spawn_multiple(tasks)?)
    }

    pub fn register_interest<T, H>(
        &self,
        source: &T, 
        handler: H,
        interest_type: InterestType,
    ) -> Result<(), PoolError> 
    where 
        T: AsRawFd,
        H: EventHandler + Send + Sync + 'static
    {
        let worker = self.get_next_worker()?;
        let event_registry = worker.event_registry();
        let callback_registry = worker.callback_registry();

        let key = event_registry.register_interest(source, interest_type)?;
        callback_registry.register(key, handler)?;

        Ok(())
    }

    pub fn submit_io_operation(
        &self,
        operation: IOOperation, 
        waker: Option<std::task::Waker>
    ) -> Result<WorkerIOSubmissionHandle, PoolError> {
        let worker = self.get_next_worker()?;
        log::info!("Submitting IO operation to worker {}", worker.id());
        Ok(worker.submit_io_operation(operation, waker)?)
    }

    pub fn submit_io_operations(
        &self,
        operations: Vec<IOOperation>, 
        waker: Option<std::task::Waker>
    ) -> Result<crate::worker::WorkerMultipleIOSubmissionHandle, PoolError> {
        let worker = self.get_next_worker()?;
        log::info!("Submitting {} IO operations to worker {}", operations.len(), worker.id());
        Ok(worker.submit_io_operations(operations, waker)?)
    }

    pub fn start(&self) -> Result<(), PoolError> {
        log::info!("Starting {} workers", self.n_threads);
        for i in 0..self.n_threads {
            let mut worker = Worker::new(i)?;
            worker.start()?;
            self.workers.insert(i, worker);
        }
        Ok(())
    }

    pub fn kill(&self) -> Result<(), PoolError> {
        log::info!("Killing all workers");
        Ok(for i in 0..self.n_threads {
            let mut worker = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(i))?;
            worker.kill()?;
        })
    }

    pub fn shutdown(&self) -> Result<(), PoolError> {
        log::info!("Shutting down all workers");
        for i in 0..self.n_threads {
            let mut worker = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(i))?;
            worker.shutdown()?;
        }
        self.join();
        Ok(())
    }

    fn join_single(&self, i: &usize) -> Result<(), PoolError> {
        let mut worker_guard = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(*i))?;
        let handle = worker_guard.take_join_handle();
        drop(worker_guard);
        if let Some(handle) = handle {
            handle.join().map_err(|_| PoolError::JoinError)?;
        }
        Ok(())
    }

    pub fn join(&self) {
        log::info!("Joining all workers");
        for i in 0..self.n_threads {
            if let Err(e) = self.join_single(&i) {
                log::warn!("Failed to join worker {i}: {e}");
            }
        }
    }

    pub fn num_threads(&self) -> usize {
        self.n_threads
    }
            
}