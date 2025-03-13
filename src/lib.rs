use std::sync::Arc;
use std::{cmp::max, os::fd::AsRawFd};
use log::error;
use once_cell::sync::Lazy;
use pool::ThreadPool;
use thiserror::Error;

mod event_poller;
mod event_handler;
mod worker;
mod event_channel;
mod task;
mod message;
mod worker_state;
mod executor;
mod event_callback_registry;
mod pool;

pub use task::Task;
pub use event_callback_registry::EventCallbackRegistry;
pub use event_callback_registry::EventCallbackData as SubscriptionManager;
pub use event_callback_registry::EventHandler;
pub use event_poller::PollEvent as Event;
pub use event_poller::InterestType;

#[macro_export]
#[allow(unused_macros)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Executor error: {0}")]
    Executor(#[from] executor::ExecutorError),

    #[error("Pool error: {0}")]
    Pool(#[from] pool::PoolError),

    #[error("Failed to acquire pool lock")]
    LockError,
}

pub type Result<T> = std::result::Result<T, Error>;

static POOL: Lazy<ThreadPool> = Lazy::new(|| {
    let pool = ThreadPool::new(max(2, num_cpus::get()));
    pool.start().expect("Failed to start thread pool");
    pool
});

pub (crate) fn get_stealers_rand(worker_id: usize) -> Arc<Vec<worker::WorkStealer>>
{ POOL.get_stealers_rand(worker_id) }

// Schedules a new task to be executed by the thread pool.
pub fn spawn<F>(fut: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static
{ Ok(POOL.spawn(Task::new(fut))?) }

// Registers interest in a file descriptor for events.
pub fn register_interest<T, H>(
    source: &T, 
    interest_type: event_poller::InterestType,
    handler: H
) -> Result<()> 
where 
    T: AsRawFd,
    H: event_callback_registry::EventHandler + Send + Sync + 'static
{
    Ok(POOL.register_interest(source, handler, interest_type)?)
}

// Blocks the current thread until all pool threads have been stopped.
pub fn join() -> Result<()> { Ok(POOL.join()?) }

// Gracefully stops all threads in the pool, allowing all tasks to finish.
pub fn shutdown() -> Result<()> { Ok(POOL.shutdown()?) }

// Immediately stops all threads in the pool, ignoring all tasks.
pub fn kill() -> Result<()> { Ok(POOL.kill()?) }

// Returns the number of threads in the pool.
// This should always equal the number of CPUs.
pub fn num_threads() -> Result<usize> { Ok(POOL.num_threads()) }