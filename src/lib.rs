#![feature(decl_macro)]

use once_cell::sync::OnceCell;
use pool::PoolError;
use thiserror::Error;

#[macro_use]
pub (crate) mod sys;

#[macro_use]
pub (crate) mod logging;

mod task;
mod worker;
mod key;
mod os_error;
mod pool;

pub mod io;
pub mod events;
pub mod sync;
pub mod timing;
pub mod net;
pub mod notifications;

pub use key::Key;
pub use task::Task;

pub (crate) use worker::Worker;
pub (crate) use pool::ThreadPool;
pub use os_error::OsError;


#[derive(Debug, Error)]
pub enum Error {
    #[error("Executor error: {0}")]
    Executor(#[from] task::executor::ExecutorError),

    #[error("Pool error: {0}")]
    Pool(#[from] pool::PoolError),

    #[error("OS error: {0}")]
    Os(#[from] OsError),

    #[error("IO error: {0}")]
    Io(#[from] io::IoError),

    #[error("Network error: {0}")]
    Network(#[from] net::NetworkError),

    #[error("Failed to acquire pool lock")]
    LockError,

    #[error("Pool already initialized")]
    AlreadyInitialized,
}

pub type Result<T> = std::result::Result<T, Error>;

static POOL: OnceCell<ThreadPool> = OnceCell::new();

fn pool() -> &'static ThreadPool {
    POOL.get_or_init(|| {
        ThreadPool::new(num_cpus::get())
        .start()
        .expect("Failed to start thread pool")
    })
}

// Initializes the thread pool with the specified number of threads.
// cl_async requires a minimum of 2 threads.
pub fn init(n_threads: usize) -> Result<()> {
    if POOL.get().is_some() { return Err(Error::AlreadyInitialized); }
    POOL.set(
        ThreadPool::new(n_threads)
        .start()
        .expect("Failed to start thread pool")
    ).map_err(|_| Error::LockError)?;
    Ok(())
}

// Schedules a new task to be executed by the thread pool.
pub fn spawn<F>(fut: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static
{ Ok(pool().spawn(Task::new(fut))?) }

// Schedules multiple tasks to be executed
pub fn spawn_multiple<F>(fut: Vec<F>) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static
{ 
    let mut tasks = Vec::with_capacity(fut.len());
    for fut in fut { tasks.push(Task::new(fut)); }
    Ok(pool().spawn_multiple(tasks)?)
}

// Blocks the current thread until all pool threads have been stopped.
pub fn join() { pool().join() }

// Gracefully stops all threads in the pool, allowing all tasks to finish.
pub fn shutdown() -> Result<()> { Ok(pool().shutdown()?) }

// Immediately stops all threads in the pool, ignoring all tasks.
pub fn kill() -> Result<()> { Ok(pool().kill()?) }

// Returns the number of threads in the pool.
// This should always equal the number of CPUs.
pub fn num_threads() -> usize { pool().num_threads() }

// Submits an I/O operation to the pool.
pub fn submit_io_operation(
    operation: io::IoOperation, 
    waker: Option<std::task::Waker>
) -> std::result::Result<worker::WorkerIOSubmissionHandle, PoolError> {
    Ok(pool().submit_io_operation(operation, waker)?)
}

// Submits multiple I/O operations to the pool.
pub fn submit_io_operations(
    operations: Vec<io::IoOperation>, 
    waker: Option<std::task::Waker>
) -> std::result::Result<worker::WorkerMultipleIOSubmissionHandle, PoolError> {
    Ok(pool().submit_io_operations(operations, waker)?)
}

pub fn register_event_source<F, Fut>(
    source: events::EventSource,
    interest_type: events::InterestType,
    handler: F
) -> Result<()>
where
    F: FnOnce(events::EventReceiver) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static
{ Ok(pool().register_event_source(source, interest_type, handler)?) }

pub fn sleep(duration: std::time::Duration) -> timing::futures::SleepFuture {
    timing::futures::SleepFuture::new(duration)
}

pub fn get_worker_handle(id: usize) -> Result<worker::WorkerHandle> {
    Ok(pool().get_worker_handle(id)?)
}

pub fn next_worker_id() -> usize {
    pool().next_worker_id()
}

pub fn notify_on(
    flags: notifications::NotificationFlags,
) -> Result<notifications::Subscription> {
    Ok(pool().notify_on(flags)?)
}