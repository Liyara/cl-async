use std::{num::NonZero, pin::Pin, sync::{atomic::AtomicUsize, Arc}, task::{Context, Poll, Waker}};

pub struct Permit<'a> {
    inner: &'a SemaphoreInner,
    weight: usize,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.inner.add_permits(self.weight);
    }
}

pub struct OwnedPermit {
    inner: Arc<SemaphoreInner>,
    weight: usize
}

impl Drop for OwnedPermit {
    fn drop(&mut self) {
        self.inner.add_permits(self.weight);
    }
}

fn sem_fut<F: FnOnce(usize) -> P, P>(
    f: F, 
    cx: &mut Context<'_>, 
    inner: &SemaphoreInner, 
    desired_permits: usize, 
    state: &mut SemaphoreFutureState
) -> Poll<P> {

    let mut queued = false;

    loop {
        
        let needed = desired_permits.saturating_sub(match state {
            SemaphoreFutureState::Polite => 0,
            SemaphoreFutureState::Hungry(count) => *count,
        });

        let permits = inner.permits.load(std::sync::atomic::Ordering::Acquire);
    
        if permits >= needed {
            match inner.permits.compare_exchange(
                permits,
                permits - needed,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            ) {
                Ok(_) => return Poll::Ready(f(desired_permits)),
                Err(_) => continue, // Some spinning may occur
            }
        }

        if let SemaphoreFutureState::Hungry(count) = state {
            if permits > 0 {
                match inner.permits.compare_exchange(
                    permits,
                    0,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                ) {
                    Ok(n) => *count += n,
                    Err(_) => continue, // Some spinning may occur
                }
            }
        }

        if !queued {
            inner.waker_queue.push(cx.waker().clone());
            queued = true;
            continue;
        }

        return Poll::Pending;
    }
}

enum SemaphoreFutureState {
    Polite,
    Hungry(usize)
}

pub struct SemaphoreFuture<'a> {
    inner: &'a SemaphoreInner,
    desired_permits: usize,
    state: SemaphoreFutureState,
}

impl<'a> SemaphoreFuture<'a> {

    pub (crate) fn new(inner: &'a SemaphoreInner, desired_permits: usize) -> Self {
        Self {
            inner,
            desired_permits,
            state: SemaphoreFutureState::Polite,
        }
    }

    pub (crate) fn new_hungry(inner: &'a SemaphoreInner, desired_permits: usize) -> Self {
        Self {
            inner,
            desired_permits,
            state: SemaphoreFutureState::Hungry(0),
        }
    }
}

impl<'a> Future for SemaphoreFuture<'a> {
    type Output = Permit<'a>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
       
        let this = self.as_mut().get_mut();
        let inner = this.inner;
        let desired_permits = this.desired_permits;
        let state = &mut this.state;
        
        sem_fut(| permits | Permit {
            inner,
            weight: permits,
        }, cx, inner, desired_permits, state)

    }
}

impl Drop for SemaphoreFuture<'_> {
    fn drop(&mut self) {
        if let SemaphoreFutureState::Hungry(count) = &mut self.state {
            if *count > 0 {
                self.inner.add_permits(*count);
            }
        }
    }
}

pub struct OwnedSemaphoreFuture {
    inner: Arc<SemaphoreInner>,
    desired_permits: usize,
    state: SemaphoreFutureState,
}

impl OwnedSemaphoreFuture {

    pub (crate) fn new(inner: Arc<SemaphoreInner>, desired_permits: usize) -> Self {
        Self {
            inner,
            desired_permits,
            state: SemaphoreFutureState::Polite,
        }
    }

    pub (crate) fn new_hungry(inner: Arc<SemaphoreInner>, desired_permits: usize) -> Self {
        Self {
            inner,
            desired_permits,
            state: SemaphoreFutureState::Hungry(0),
        }
    }
}

impl Future for OwnedSemaphoreFuture {
    type Output = OwnedPermit;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        
        let this = self.as_mut().get_mut();
        let inner = &this.inner;
        let desired_permits = this.desired_permits;
        let state = &mut this.state;
        
        sem_fut(| permits | OwnedPermit {
            inner: inner.clone(),
            weight: permits,
        }, cx, inner, desired_permits, state)

    }
}

impl Drop for OwnedSemaphoreFuture {
    fn drop(&mut self) {
        if let SemaphoreFutureState::Hungry(count) = &mut self.state {
            if *count > 0 {
                self.inner.add_permits(*count);
            }
        }
    }
}

#[derive(Clone)]
pub struct Semaphore {
    inner: Arc<SemaphoreInner>,
}

impl Semaphore {

    #[inline]
    pub fn new(permits: usize) -> Self {
        Self {
            inner: Arc::new(SemaphoreInner::new(permits)),
        }
    }

    #[inline]
    pub fn acquire<'a>(&'a self) -> SemaphoreFuture<'a> {
        self.acquire_many(unsafe { NonZero::new_unchecked(1) })
    }

    #[inline]
    pub fn acquire_owned(&self) -> OwnedSemaphoreFuture {
        self.acquire_many_owned(unsafe { NonZero::new_unchecked(1) })
    }

    #[inline]
    pub fn acquire_many<'a>(&'a self, n: NonZero<usize>) -> SemaphoreFuture<'a> {
        SemaphoreFuture::new(&self.inner, n.get())
    }

    #[inline]
    pub fn acquire_many_owned(&self, n: NonZero<usize>) -> OwnedSemaphoreFuture {
        OwnedSemaphoreFuture::new(self.inner.clone(), n.get())
    }

    #[inline]
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    #[inline]
    pub unsafe fn acquire_unchecked<'a>(&'a self) -> SemaphoreFuture<'a> {
        SemaphoreFuture::new(&self.inner, 1)
    }

    #[inline]
    pub unsafe fn acquire_many_unchecked<'a>(&'a self, n: NonZero<usize>) -> SemaphoreFuture<'a> {
        SemaphoreFuture::new(&self.inner, n.get())
    }

    #[inline]
    pub unsafe fn acquire_owned_unchecked(&self) -> OwnedSemaphoreFuture {
        OwnedSemaphoreFuture::new(self.inner.clone(), 1)
    }

    #[inline]
    pub unsafe fn acquire_many_owned_unchecked(&self, n: NonZero<usize>) -> OwnedSemaphoreFuture {
        OwnedSemaphoreFuture::new(self.inner.clone(), n.get())
    }

    #[inline]
    pub fn acquire_hungry<'a>(&'a self, n: NonZero<usize>) -> SemaphoreFuture<'a> {
        SemaphoreFuture::new_hungry(&self.inner, n.get())
    }

    #[inline]
    pub fn acquire_owned_hungry(&self, n: NonZero<usize>) -> OwnedSemaphoreFuture {
        OwnedSemaphoreFuture::new_hungry(self.inner.clone(), n.get())
    }

    #[inline]
    pub unsafe fn acquire_hungry_unchecked<'a>(&'a self, n: usize) -> SemaphoreFuture<'a> {
        SemaphoreFuture::new_hungry(&self.inner, n)
    }

    #[inline]
    pub unsafe fn acquire_owned_hungry_unchecked(&self, n: usize) -> OwnedSemaphoreFuture {
        OwnedSemaphoreFuture::new_hungry(self.inner.clone(), n)
    }
}

pub (crate) struct SemaphoreInner {
    permits: AtomicUsize,
    waker_queue: crossbeam_queue::SegQueue<Waker>
}

impl SemaphoreInner {

    pub const fn new(permits: usize) -> Self {
        Self {
            permits: AtomicUsize::new(permits),
            waker_queue: crossbeam_queue::SegQueue::new(),
        }
    }

    pub fn available_permits(&self) -> usize {
        self.permits.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn add_permits(&self, n: usize) {
        self.permits.fetch_add(n, std::sync::atomic::Ordering::AcqRel);
        for _ in 0..n {
            if let Some(waker) = self.waker_queue.pop() {
                waker.wake();
            } else {
                break;
            }
        }
    }

    pub fn acquire(&self, n: usize) -> SemaphoreFuture<'_> {
        SemaphoreFuture::new(self, n)
    }

    pub fn acquire_hungry(&self, n: usize) -> SemaphoreFuture<'_> {
        SemaphoreFuture::new_hungry(self, n)
    }
}