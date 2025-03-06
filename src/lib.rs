use std::sync::Mutex;

use log::error;
use once_cell::sync::Lazy;
use pool::ThreadPool;
use task::Task;
use thiserror::Error;

mod task;
mod executor;
mod pool;
mod message;
mod worker;
mod worker_state;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Executor error: {0}")]
    Executor(#[from] executor::ExecutorError),

    #[error("Pool error: {0}")]
    Pool(#[from] pool::PoolError),

    #[error("Worker error: {0}")]
    Worker(#[from] worker::WorkerError),

    #[error("Failed to acquire pool lock")]
    LockError,
}

pub type Result<T> = std::result::Result<T, Error>;

static POOL: Lazy<Mutex<ThreadPool>> = Lazy::new(|| {
    let mut pool = ThreadPool::new(num_cpus::get());
    match pool.start() {
        Ok(_) => (),
        Err(e) => error!("Failed to start pool: {}", e),
    }
    Mutex::new(pool)
});

pub fn spawn<F>(fut: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static
{
    let task = Task::new(fut);
    POOL.lock().map_err(|_| Error::LockError)?.spawn(task)?;
    Ok(())
}

// Blocks the current thread until all pool threads have been stopped.
pub fn join() -> Result<()> {
    POOL.lock().map_err(|_| Error::LockError)?.join()?;
    Ok(())
}

// Gracefully stops all threads in the pool, allowing all tasks to finish.
pub fn shutdown() -> Result<()> {
   POOL.lock().map_err(|_| Error::LockError)?.shutdown()?;
   Ok(())
}

// Immediately stops all threads in the pool, ignoring all tasks.
pub fn kill() -> Result<()> {
    POOL.lock().map_err(|_| Error::LockError)?.kill();
    Ok(())
}

pub fn num_threads() -> Result<usize> {
    let n = POOL.lock().map_err(|_| Error::LockError)?.num_threads();
    Ok(n)
}