use std::sync::atomic::AtomicUsize;
use dashmap::DashMap;
use thiserror::Error;

use crate::{
    events::{
        poller::{
            registry::EventPollerRegistryError, 
            EventPollerError, 
            InterestType
        }, 
        EventQueue, 
        EventReceiver, 
        EventSource
    }, io::IoOperation, notifications:: NotificationFlags, worker::{
        WorkerError, WorkerHandle, WorkerIOSubmissionHandle, WorkerState
    }, Task, Worker
};

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Worker {0} not found")]
    WorkerNotFound(usize),

    #[error("Woker error: {0}")]
    WorkerError(#[from] WorkerError),

    #[error("Event Poller error: {0}")]
    EventPollerError(#[from] EventPollerError),

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

        let n_threads = std::cmp::max(2, n_threads);

        Self { 
            n_threads, 
            workers: DashMap::new()
        }
    }

    pub fn next_worker_id(&'static self) -> usize {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        id % self.n_threads
    }

    fn get_next_worker(
        &'static self
    ) -> Result<dashmap::mapref::one::RefMut<'static, usize, Worker>, PoolError> {
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

    pub fn spawn(&'static self, task: Task) -> Result<(), PoolError> {
        info!("cl-async: Spawning task with ID {}", task.id);
        Ok(self.get_next_worker()?.spawn(task)?)
    }

    pub fn spawn_multiple(&'static self, tasks: Vec<Task>) -> Result<(), PoolError> {
        info!("cl-async: Spawning {} tasks", tasks.len());
        Ok(self.get_next_worker()?.spawn_multiple(tasks)?)
    }

    pub fn get_worker_handle(&'static self, id: usize) -> Result<WorkerHandle, PoolError> {
        let worker = self.workers.get(&id).ok_or(PoolError::WorkerNotFound(id))?;
        Ok(worker.as_handle())
    }

    pub fn register_event_source<F, Fut>(
        &'static self,
        source: EventSource,
        interest_type: InterestType,
        handler: F
    ) -> Result<(), PoolError>
    where
        F: FnOnce(crate::events::EventReceiver) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static
    {
        info!("cl-async: Registering event source {}", source);

        let worker = self.get_next_worker()?;
        let event_registry = worker.event_registry().clone();
        let event_queue_registry = worker.queue_registry().clone();

        let task = Task::new(async move {

            let key = match event_registry.register_interest(source, interest_type) {
                Ok(key) => key,
                Err(e) => {
                    error!("cl-async: Failed to register interest: {}", e);
                    return;
                }
            };

            let event_queue = EventQueue::new();
            event_queue_registry.register(key, event_queue.clone());
            let event_receiver = EventReceiver::new(
                source,
                key,
                event_queue,
                event_registry.clone(),
            );

            handler(event_receiver).await;

            let _ = event_registry.deregister_interest(key);
            event_queue_registry.remove(key);
        });

        worker.spawn(task)?;
        Ok(())
    }

    pub fn submit_io_operation(
        &'static self,
        operation: IoOperation, 
        waker: Option<std::task::Waker>
    ) -> Result<WorkerIOSubmissionHandle, PoolError> {
        let worker = self.get_next_worker()?;
        info!("cl-async: Submitting IO operation to worker {}", worker.id());
        Ok(worker.submit_io_operation(operation, waker)?)
    }

    pub fn submit_io_operations(
        &'static self,
        operations: Vec<IoOperation>, 
        waker: Option<std::task::Waker>
    ) -> Result<crate::worker::WorkerMultipleIOSubmissionHandle, PoolError> {
        let worker = self.get_next_worker()?;
        info!("cl-async: Submitting {} IO operations to worker {}", operations.len(), worker.id());
        Ok(worker.submit_io_operations(operations, waker)?)
    }

    pub fn start(self) -> Result<Self, PoolError> {
        info!("cl-async: Starting {} workers", self.n_threads);
        for i in 0..self.n_threads {
            let mut worker = Worker::new(i)?;
            worker.start()?;
            self.workers.insert(i, worker);
        }
        Ok(self)
    }

    pub fn kill(&'static self) -> Result<(), PoolError> {
        info!("cl-async: Killing all workers");
        Ok(for i in 0..self.n_threads {
            let worker = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(i))?;
            worker.kill()?;
        })
    }

    pub fn shutdown(&'static self) -> Result<(), PoolError> {
        info!("cl-async: Shutting down all workers");
        for i in 0..self.n_threads {
            let worker = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(i))?;
            worker.shutdown()?;
        }
        self.join();
        Ok(())
    }

    fn join_single(&'static self, i: &usize) -> Result<(), PoolError> {
        let mut worker_guard = self.workers.get_mut(&i).ok_or(PoolError::WorkerNotFound(*i))?;
        let handle = worker_guard.take_join_handle();
        drop(worker_guard);
        if let Some(handle) = handle {
            handle.join().map_err(|_| PoolError::JoinError)?;
        }
        Ok(())
    }

    pub fn join(&'static self) {
        info!("cl-async: Joining all workers");
        for i in 0..self.n_threads {
            if let Err(e) = self.join_single(&i) {
                warn!("cl-async: Failed to join worker {i}: {e}");
            }
        }
    }

    pub fn num_threads(&'static self) -> usize {
        self.n_threads
    }

    pub fn notify_on(
        &'static self,
        flags: NotificationFlags
    ) -> Result<crate::notifications::Subscription, PoolError> {
        let worker = self.get_next_worker()?;
        Ok(worker.notify_on(flags)?)
    }
            
}