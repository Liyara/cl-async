pub mod registry;
pub mod futures;

use std::{
    cell::UnsafeCell, 
    collections::VecDeque, 
    rc::Rc, 
    task::Waker
};

use crate::events::Event;

macro_rules! inner {
    ($self:ident) => {
        unsafe { &mut *$self.inner.get() }
    };
}

use super::futures::NextEventFuture;
#[derive(Debug)]
struct EventQueueInner {
    queue: VecDeque<Event>,
    waker_queue: VecDeque<Waker>,
}

#[derive(Debug, Clone)]
pub struct EventQueue {
    inner: Rc<UnsafeCell<EventQueueInner>>,
}

impl EventQueue {

    pub fn new() -> Self {
        Self {
            inner: Rc::new(UnsafeCell::new(EventQueueInner {
                queue: VecDeque::new(),
                waker_queue: VecDeque::new(),
            })),
        }
    }

    pub fn push(&self, event: Event) {
        inner!(self).queue.push_back(event);
    }

    pub fn wake(&self) {
        while let Some(waker) = inner!(self).waker_queue.pop_front() {
            waker.wake();
        }
    }

    pub fn pop(&self) -> Option<Event> {
        inner!(self).queue.pop_front()
    }

    pub fn len(&self) -> usize {
        inner!(self).queue.len()
    }

    pub fn is_empty(&self) -> bool {
        inner!(self).queue.is_empty()
    }

    pub fn register_waker(&self, waker: &std::task::Waker) {
        inner!(self).waker_queue.push_back(waker.clone());
    }

    pub fn next_event<'a, 'b>(&'a self) -> NextEventFuture<'b> 
    where 'a: 'b {
        NextEventFuture {
            event_queue: self,
        }
    }

}

unsafe impl Send for EventQueue {}
unsafe impl Sync for EventQueue {}