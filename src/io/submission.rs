use crate::Key;

use super::{
    IoEntry, 
    IoOperation
};

pub struct IoSubmission {
    pub key: Key,
    pub op: IoOperation,
    pub waker: Option<std::task::Waker>,
}

impl IoSubmission {

    pub fn split(self) -> (IoEntry, Option<std::task::Waker>) {
        let waker = self.waker;
        let entry = IoEntry {
            key: self.key,
            op: self.op,
        };
        (entry, waker)
    }
}