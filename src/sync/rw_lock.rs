use std::{cell::UnsafeCell, num::NonZeroUsize, ops::{Deref, DerefMut}};

use crate::sync::semaphore::{Permit, SemaphoreInner};

pub struct ReadGuard<'a, T> {
    data: &'a T,
    _pemit: Permit<'a>
}

impl<'a, T> Deref for ReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

unsafe impl<T: Send> Send for ReadGuard<'_, T> {}
unsafe impl<T: Sync> Sync for ReadGuard<'_, T> {}

pub struct WriteGuard<'a, T> {
    data: &'a mut T,
    _pemit: Permit<'a>
}

impl<'a, T> Deref for WriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, T> DerefMut for WriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

unsafe impl<T: Send> Send for WriteGuard<'_, T> {}
unsafe impl<T: Sync> Sync for WriteGuard<'_, T> {}

pub struct RwLock<T> {
    max_permits: usize,
    semaphore: SemaphoreInner,
    data: UnsafeCell<T>
}

unsafe impl<T: Send> Send for RwLock<T> {}
unsafe impl<T: Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {

    pub fn new(data: T) -> Self {
        Self::with_max_readers(
            data, 
            unsafe { NonZeroUsize::new_unchecked(usize::MAX) }
        )
    }

    pub fn with_max_readers(data: T, max_readers: NonZeroUsize) -> Self {
        let max_readers = max_readers.get();
        Self {
            max_permits: max_readers,
            semaphore: SemaphoreInner::new(max_readers),
            data: UnsafeCell::new(data),
        }
    }

    pub async fn read<'a>(&'a self) -> ReadGuard<'a, T> {
        let permit = self.semaphore.acquire(1).await;
        ReadGuard {
            data: unsafe { &*self.data.get() },
            _pemit: permit
        }
    }

    pub async fn write<'a>(&'a self) -> WriteGuard<'a, T> {
        let permit = self.semaphore.acquire_hungry(self.max_permits).await;
        WriteGuard {
            data: unsafe { &mut *self.data.get() },
            _pemit: permit
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}