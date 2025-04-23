use std::task::Context;

use super::{Notification, NotificationError, NotificationResult};
use bitflags::bitflags;
use futures::StreamExt;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NotificationFlags: u64 {
        const SHUTDOWN = 1 << 0;
        const KILL = 1 << 1;
        const CONTINUE = 1 << 2;
        const CHILD_STOPPED = 1 << 3;
        const CUSTOM = 1 << 4;
    }
}

impl NotificationFlags {
    pub fn matches(&self, notification: &Notification) -> bool {
        match notification {
            Notification::Shutdown => self.contains(NotificationFlags::SHUTDOWN),
            Notification::Kill => self.contains(NotificationFlags::KILL),
            Notification::Continue => self.contains(NotificationFlags::CONTINUE),
            Notification::ChildStopped => self.contains(NotificationFlags::CHILD_STOPPED),
            Notification::Custom(_) => self.contains(NotificationFlags::CUSTOM),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Subscription {
    receiver: async_broadcast::Receiver<Notification>,
    flags: NotificationFlags,
}

impl Subscription {
    pub (crate) fn new(
        receiver: async_broadcast::Receiver<Notification>,
        flags: NotificationFlags,
    ) -> Self {
        Self { receiver, flags }
    }

    pub async fn recv(&mut self) -> NotificationResult<Notification> {
        NotificationFuture::new(
            &mut self.receiver, 
            self.flags
        ).await
    }

    pub fn flags(&self) -> NotificationFlags { self.flags }
    pub fn update_flags(&mut self, flags: NotificationFlags) { self.flags = flags; }
}

struct NotificationFuture<'a> {
    receiver: &'a mut async_broadcast::Receiver<Notification>,
    flags: NotificationFlags,
}

impl<'a> NotificationFuture<'a> {
    pub fn new(
        receiver: &'a mut async_broadcast::Receiver<Notification>,
        flags: NotificationFlags,
    ) -> Self {
        Self { receiver, flags }
    }
}

impl<'a> std::future::Future for NotificationFuture<'a> {
    type Output = NotificationResult<Notification>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.as_mut().get_mut();

        loop {
            match this.receiver.poll_next_unpin(cx) {
                std::task::Poll::Ready(Some(notification)) => {
                    if this.flags.matches(&notification) {
                        return std::task::Poll::Ready(Ok(notification));
                    }
                },
                std::task::Poll::Ready(None) => {
                    return std::task::Poll::Ready(Err(NotificationError::StreamClosed));
                },
                std::task::Poll::Pending => {
                    return std::task::Poll::Pending;
                }
            }
        }

    }
}