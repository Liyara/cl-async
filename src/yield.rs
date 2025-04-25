use std::{sync::atomic::AtomicBool, task::Poll};


pub struct YieldFuture {
    yielded: AtomicBool,
}

impl YieldFuture {
    pub fn new() -> Self {
        Self {
            yielded: AtomicBool::new(false),
        }
    }
}

impl std::future::Future for YieldFuture {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        match self.yielded.compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ) {
            Ok(false) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(true) => {
                Poll::Ready(())
            }
            _ => {
                if self.yielded.load(std::sync::atomic::Ordering::SeqCst) {
                    Poll::Ready(())
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }
    }
}