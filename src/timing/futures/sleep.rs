use std::{os::fd::RawFd, sync::Arc, task::Waker, time::Duration};

use parking_lot::Mutex;
use thiserror::Error;

use crate::events::{
    EventSource, 
    InterestType
};

#[derive(Debug, Error)]
pub enum SleepError {
    #[error("Failed to create timerfd")]
    TimerCreateError(std::io::Error),

    #[error("Failed to set timerfd")]
    TimerSetError(std::io::Error),

    #[error("Failed to close timerfd: {0}")]
    TimerCloseError(std::io::Error),

    #[error("Failed to poll timerfd")]
    TimerPollError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SleepState {
    NotSubmitted,
    Pending,
    Failure,
    Complete,
}

pub struct SleepFuture {
    duration: Duration,
    state: Arc<Mutex<SleepState>>,
}

impl SleepFuture {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            state: Arc::new(Mutex::new(SleepState::NotSubmitted)),
        }
    }

    fn try_close_fd(timer_fd: RawFd) -> Result<i32, SleepError> {
        syscall!(
            close(timer_fd)
        ).map_err(SleepError::TimerCloseError)
    }

    fn close_fd(timer_fd: RawFd) -> bool {
        if let Err(e) = Self::try_close_fd(timer_fd) {
            error!("cl-async: {}", e);
            false
        } else { true }
    }

    fn set_state_and_wake(
        old_state: Arc<Mutex<SleepState>>, 
        new_state: SleepState,
        waker: Waker,
    ) {
        *old_state.lock() = new_state;
        waker.wake();
    }

    fn submit(&mut self, waker: Waker) -> Result<(), SleepError> {

        let timer_fd = syscall!(timerfd_create(
            libc::CLOCK_MONOTONIC,
            libc::TFD_NONBLOCK | libc::TFD_CLOEXEC
        )).map_err(SleepError::TimerCreateError)?;

        if let Err(e) = {

            let timer_spec = libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
                it_value: libc::timespec {
                    tv_sec: self.duration.as_secs() as i64,
                    tv_nsec: self.duration.subsec_nanos() as i64,
                },
            };

            syscall!(timerfd_settime(
                timer_fd,
                0,
                &timer_spec,
                std::ptr::null_mut()
            )).map_err(SleepError::TimerSetError)?;

            let state_clone = self.state.clone();

            crate::register_event_source(
                EventSource::new(&timer_fd),
                InterestType::READ | InterestType::ONESHOT,
                async move |event_receiver| {
                    event_receiver.next_event().await;

                    let mut buf = [0u8; 8];
                    if let Err(e) = syscall!(
                        read(timer_fd, buf.as_mut_ptr() as *mut libc::c_void, 8)
                    ) {
                        error!("cl-async: Failed to read from timerfd: {}", e);
                        Self::set_state_and_wake(
                            state_clone,
                            SleepState::Failure,
                            waker,
                        );
                        return;
                    }
                    
                    if !Self::close_fd(timer_fd) {
                        Self::set_state_and_wake(
                            state_clone,
                            SleepState::Failure,
                            waker,
                        );
                        return;
                    }

                    Self::set_state_and_wake(
                        state_clone,
                        SleepState::Complete,
                        waker,
                    );
                }
            ).map_err(|_| SleepError::TimerPollError)?;

            *self.state.lock() = SleepState::Pending;

            Ok(())
        } {
            Self::try_close_fd(timer_fd)?;
            *self.state.lock() = SleepState::Failure;
            Err(e)
        } else { Ok(()) }

    }

}

impl std::future::Future for SleepFuture {
    type Output = Result<(), SleepError>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context) -> std::task::Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        let state = *this.state.lock();

        match state {
            SleepState::NotSubmitted => {
                this.submit(cx.waker().clone())?;
                std::task::Poll::Pending
            },
            SleepState::Pending => {
                std::task::Poll::Pending
            },
            SleepState::Complete => {
                std::task::Poll::Ready(Ok(()))
            },
            SleepState::Failure => {
                std::task::Poll::Ready(Err(SleepError::TimerPollError))
            }
        }
    }
}