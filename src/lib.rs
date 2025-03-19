use std::{cmp::max, os::fd::AsRawFd};

use once_cell::sync::Lazy;
use thiserror::Error;

mod events;
mod task;
mod worker;
mod key;
mod os_error;
mod pool;
mod timing;

pub mod io;

pub use key::Key;

pub (crate) use events::Event;
pub (crate) use task::Task;
pub (crate) use worker::Worker;
pub (crate) use pool::ThreadPool;
pub (crate) use os_error::OSError;

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
    Executor(#[from] task::executor::ExecutorError),

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

// Schedules a new task to be executed by the thread pool.
pub fn spawn<F>(fut: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static
{ Ok(POOL.spawn(Task::new(fut))?) }

pub fn spawn_multiple<F>(fut: Vec<F>) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static
{ 
    let mut tasks = Vec::with_capacity(fut.len());
    for fut in fut { tasks.push(Task::new(fut)); }
    Ok(POOL.spawn_multiple(tasks)?)
}

// Registers interest in a file descriptor for events.
pub fn register_interest<T, H>(
    source: &T, 
    interest_type: events::poller::InterestType,
    handler: H
) -> Result<()> 
where 
    T: AsRawFd,
    H: events::EventHandler + Send + Sync + 'static
{
    Ok(POOL.register_interest(source, handler, interest_type)?)
}

// Blocks the current thread until all pool threads have been stopped.
pub fn join() { POOL.join() }

// Gracefully stops all threads in the pool, allowing all tasks to finish.
pub fn shutdown() -> Result<()> { Ok(POOL.shutdown()?) }

// Immediately stops all threads in the pool, ignoring all tasks.
pub fn kill() -> Result<()> { Ok(POOL.kill()?) }

// Returns the number of threads in the pool.
// This should always equal the number of CPUs.
pub fn num_threads() -> Result<usize> { Ok(POOL.num_threads()) }

pub (crate) fn submit_io_operation(
    operation: io::IOOperation, 
    waker: Option<std::task::Waker>
) -> Result<worker::WorkerIOSubmissionHandle> {
    Ok(POOL.submit_io_operation(operation, waker)?)
}

pub (crate) fn submit_io_operations(
    operations: Vec<io::IOOperation>, 
    waker: Option<std::task::Waker>
) -> Result<worker::WorkerMultipleIOSubmissionHandle> {
    Ok(POOL.submit_io_operations(operations, waker)?)
}