use std::sync::atomic::AtomicUsize;
use dashmap::DashMap;
use thiserror::Error;

use crate::{
    events::{
        poller::InterestType, 
        EventQueue, 
        EventReceiver, 
        EventSource
    }, 
    io::IoOperation, 
    notifications::NotificationFlags, 
    worker::{
        work_sender::SendToWorkerChannelError, 
        Message,
        WorkerHandle, 
        WorkerIOSubmissionHandle, 
        WorkerInitializationError, 
        WorkerStartError, 
        WorkerState
    }, 
    Task, 
    Worker
};

#[derive(Debug, Error)]
#[error("Failed to locate any available workers")]
pub struct NextWorkerError;

#[derive(Debug, Error)]
pub enum SpawnTaskErrorKind {
    #[error("Couldn't get worker for task: {0}")]
    FailedToGetWorker(#[from] NextWorkerError),

    #[error("Failed to send task to worker")]
    FailedToSendTask,
}

#[derive(Debug, Error)]
#[error("Failed to spawn task: {kind}")]
pub struct SpawnTaskError {

    pub task: Task,

    #[source]
    pub kind: SpawnTaskErrorKind,
}

impl SpawnTaskError {
    pub fn failed_to_get_worker(task: Task) -> Self {
        Self {
            task,
            kind: SpawnTaskErrorKind::FailedToGetWorker(NextWorkerError)
        }
    }

    pub fn failed_to_send_task(task: Task) -> Self {
        Self {
            task,
            kind: SpawnTaskErrorKind::FailedToSendTask
        }
    }
}

#[derive(Debug, Error)]
pub enum WorkerDispatchError {
    #[error("Failed to find suitable worker: {0}")]
    FailedToFindWorker(#[from] NextWorkerError),

    #[error("Failed to send task to worker: {0}")]
    FailedToSendMessage(#[from] SendToWorkerChannelError),
}

#[derive(Debug, Error)]
pub enum PoolStartError {

    #[error("Failed to initialize worker: {0}")]
    WorkerInitError(#[from] WorkerInitializationError),

    #[error("Failed to start worker: {0}")]
    WorkerStartError(#[from] WorkerStartError), 
}

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Failed to start pool: {0}")]
    StartError(#[from] PoolStartError),

    #[error("Failed to dispatch message to worker: {0}")]
    DispatchError(#[from] WorkerDispatchError),

    #[error("Failed to spawn task: {0}")]
    SpawnError(#[from] SpawnTaskError),

    #[error("Failed to find any available workers: {0}")]
    NoAvailableWorkers(#[from] NextWorkerError),
}

pub struct ThreadPool {
    n_threads: usize,
    workers: DashMap<usize, Worker>,
}

impl ThreadPool {

    pub (crate) fn new(n_threads: usize) -> Self {

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
    ) -> Result<dashmap::mapref::one::RefMut<'static, usize, Worker>, NextWorkerError> {
        for _ in 0..self.n_threads {
            
            let next_worker_id = self.next_worker_id();

            let worker = match self.workers.get_mut(
                &next_worker_id
            ) {
                Some(worker) => worker,
                None => continue
            };

            let state = worker.get_state();

            if 
                state == WorkerState::None ||
                state == WorkerState::Stopped ||
                state == WorkerState::Stopping
            { continue }

            return Ok(worker);
        }
        Err(NextWorkerError)
    }

    pub fn spawn(&'static self, task: Task) -> Result<(), SpawnTaskError> {
        info!("cl-async: Spawning task with ID {}", task.id);

        let worker = match self.get_next_worker() {
            Ok(worker) => worker,
            Err(_) => return Err(SpawnTaskError::failed_to_get_worker(task))
        };

        if let Err(e) = worker.spawn(task) {
            match e.into_message() {
                Message::SpawnTask(task) => {
                    return Err(SpawnTaskError::failed_to_send_task(task));
                },
                _ => unreachable!()
            }
        }

        Ok(())
                
    }

    pub fn get_worker_handle(&'static self, id: usize) -> Option<WorkerHandle> {
        let worker = self.workers.get(&id);
        worker.map(|w| w.as_handle())
    }

    pub fn register_event_source<F, Fut>(
        &'static self,
        source: EventSource,
        interest_type: InterestType,
        handler: F
    ) -> Result<(), WorkerDispatchError>
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
    ) -> Result<WorkerIOSubmissionHandle, WorkerDispatchError> {
        let worker = self.get_next_worker()?;
        info!("cl-async: Submitting IO operation to worker {}", worker.id());
        Ok(worker.submit_io_operation(operation, waker)?)
    }

    pub fn start(self) -> Result<Self, PoolStartError> {
        info!("cl-async: Starting {} workers", self.n_threads);
        for i in 0..self.n_threads {
            let mut worker = Worker::new(i)?;
            worker.start()?;
            self.workers.insert(i, worker);
        }
        Ok(self)
    }

    pub fn kill(&'static self) {
        info!("cl-async: Killing all workers");
        for worker in self.workers.iter() {
            worker.kill().unwrap_or_else(|e| {
                warn!("cl-async: Failed to kill worker {}: {}", worker.id(), e);
            });
        }
    }

    pub fn shutdown(&'static self) {
        info!("cl-async: Shutting down all workers");
        for worker in self.workers.iter() {
            worker.shutdown().unwrap_or_else(|e| {
                warn!("cl-async: Failed to shutdown worker {}: {}", worker.id(), e);
            });
        }
        self.join();
    }

    fn join_single(&'static self, i: &usize)  {
        let mut worker_guard = match self.workers.get_mut(&i) {
            Some(worker) => worker,
            None => {
                warn!("cl-async: Worker {i} not found");
                return;
            }
        };
        let handle = worker_guard.take_join_handle();
        drop(worker_guard);
        if let Some(handle) = handle {
            handle.join().unwrap_or_else(|_| {
                warn!("cl-async: Failed to join worker {i}");
            });
        }
    }

    pub fn join(&'static self) {
        info!("cl-async: Joining all workers");
        for i in 0..self.n_threads {
            self.join_single(&i);
        }
    }

    pub fn num_threads(&'static self) -> usize {
        self.n_threads
    }

    pub fn notify_on(
        &'static self,
        flags: NotificationFlags
    ) -> Result<crate::notifications::Subscription, NextWorkerError> {
        let worker = self.get_next_worker()?;
        Ok(worker.notify_on(flags))
    }
            
}