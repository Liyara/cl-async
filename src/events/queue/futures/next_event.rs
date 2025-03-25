use crate::events::{
    Event, 
    EventQueue
};

pub struct NextEventFuture<'q> {
    pub (crate) event_queue: &'q EventQueue,
}

impl std::future::Future for NextEventFuture<'_> {
    type Output = Event;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> std::task::Poll<Self::Output> {
        if let Some(event) = self.event_queue.pop() {
            std::task::Poll::Ready(event)
        } else {
            self.event_queue.register_waker(cx.waker());
            std::task::Poll::Pending
        }
    }
}