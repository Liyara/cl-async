use std::{
    cell::UnsafeCell, 
    ops::{
        Deref, 
        DerefMut
    }, 
    sync::{
        atomic::AtomicBool, 
        Arc
    }, 
    task::Poll
};


#[derive(Clone)]
pub struct Mutex<T> {
    inner: Arc<MutexInner<T>>,
}

unsafe impl<T: Send> Send for Mutex<T> {}
unsafe impl<T: Sync> Sync for Mutex<T> {}

pub struct MutexInner<T> {
    inner: UnsafeCell<T>,
    locked: AtomicBool,
    waker_queue: crossbeam_queue::SegQueue<std::task::Waker>,
}

impl<T> MutexInner<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            locked: AtomicBool::new(false),
            waker_queue: crossbeam_queue::SegQueue::new(),
        }
    }
}

impl<T> Mutex<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(MutexInner::new(inner)),
        }
    }

    pub fn lock(&self) -> MutexFuture<T>  {
        MutexFuture::new(self.inner.clone())
    }
}

#[derive(Clone)]
pub struct MutexGuard<T> {
    inner: Arc<MutexInner<T>>,
}

impl<T> MutexGuard<T> {

    pub fn new(inner: Arc<MutexInner<T>>) -> Self {
        Self { inner }
    }
}

impl<T> Deref for MutexGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inner.inner.get() }
    }
}

impl<T> DerefMut for MutexGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inner.inner.get() }
    }
}

impl<T> Drop for MutexGuard<T> {
    fn drop(&mut self) {
        self.inner.locked.store(false, std::sync::atomic::Ordering::Release);
        let next = self.inner.waker_queue.pop();
        if let Some(waker) = next { waker.wake(); }
    }
}

unsafe impl<T: Send> Send for MutexGuard<T> {}
unsafe impl<T: Sync> Sync for MutexGuard<T> {}

pub struct MutexFuture<T> {
    inner: Option<Arc<MutexInner<T>>>,
}

impl<T> MutexFuture<T> {
    pub fn new(inner: Arc<MutexInner<T>>) -> Self {
        Self { inner: Some(inner) }
    }
}

impl<T> Future for MutexFuture<T> {
    type Output = MutexGuard<T>;
    
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        
        let this = self.as_mut().get_mut();

        fn check_lock<T>(inner: &Arc<MutexInner<T>>) -> bool {
            inner.locked.compare_exchange(
                false, 
                true, 
                std::sync::atomic::Ordering::Acquire, 
                std::sync::atomic::Ordering::Relaxed
            ).is_ok()
        }

        /*
            SAFETY:
                This unwrap is safe because this.inner is garaunteed
                to be some until we return Ready
        */
        let inner = this.inner.as_ref().unwrap();

        // First, check if the mutex is already locked.
        if check_lock(inner) {
            return Poll::Ready(MutexGuard::new(this.inner.take().unwrap()));
        }

        // If the mutex is locked, we need to queue the current task.
        inner.waker_queue.push(cx.waker().clone());

        // Lost wakeup avoidance: check again
        if check_lock(inner) {
            // May result in a spurious wakeup, acceptable.
            return Poll::Ready(MutexGuard::new(this.inner.take().unwrap()));
        }

        Poll::Pending
    }
    
}

unsafe impl<T: Send> Send for MutexFuture<T> {}
