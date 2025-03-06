use std::{sync::{atomic::{AtomicU8, Ordering}, mpsc::{Receiver, Sender, TryRecvError}, Arc}, thread::JoinHandle};

use log::error;
use thiserror::Error;

use crate::{executor::Executor, message::Message, worker_state::WorkerState};

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Worker {0} is already running")]
    AlreadyRunning(usize),

    #[error("Worker {0} is not running")]
    NotRunning(usize),

    #[error("A threading error occurred in worker {0}")]
    ThreadError(usize),
}

pub struct Worker {
    id: usize,
    sender: Sender<Message>,
    state: Arc<AtomicU8>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub (crate) fn new(id: usize, sender: Sender<Message>) -> Self {
        Self { 
            id, 
            sender, 
            state: Arc::new(AtomicU8::new(WorkerState::None as u8)), 
            handle: None,
        }
    }

    pub (crate) fn send_message(&self, message: Message) {
        self.sender.send(message).unwrap();
    }

    pub (crate) fn _state(&self) -> WorkerState { self.state.load(Ordering::Acquire).into() }

    pub (crate) fn _id(&self) -> usize { self.id }

    pub (crate) fn start(&mut self, rx: Receiver<Message>) -> Result<&mut Self, WorkerError> {

        if self.state.compare_exchange(
            WorkerState::None as u8,
            WorkerState::Starting as u8,
            Ordering::SeqCst,
            Ordering::SeqCst
        ).is_err() { return Err(WorkerError::AlreadyRunning(self.id)) }

        let id = self.id;
        let state = self.state.clone();
        let sender = self.sender.clone();

        self.handle = Some(std::thread::spawn(move || {
            let executor = Executor::new(100, sender);
            Self::worker_loop(id, rx, state, executor);
        }));

        Ok(self)
    }

    pub (crate) fn join(&mut self) -> Result<(), WorkerError> {
        let handle_opt = self.handle.take();
        match handle_opt {
            Some(handle) => match handle.join() {
                Ok(_) => Ok(()),
                Err(_) => Err(WorkerError::ThreadError(self.id)),
            },
            None => Err(WorkerError::NotRunning(self.id)),
        }
    }

    fn process_message(
        message: Message,
        state: Arc<AtomicU8>,
        executor: &mut Executor
    ) -> bool {
        match message {
            Message::NewTask(task) => {
                executor.spawn(task).unwrap_or_else(|e| error!("{}", e));
            },
            Message::Continue => (),
            Message::Shutdown => {
                state.store(WorkerState::Stopping as u8, Ordering::Release);
            },
            Message::Kill => {
                state.store(WorkerState::Stopping as u8, Ordering::Release);
                return false;
            },
        }
        true
    }

    fn worker_loop(
        id: usize, 
        rx: Receiver<Message>,
        state: Arc<AtomicU8>,
        mut executor: Executor,
    ) {
        while state.load(Ordering::Acquire) != WorkerState::Stopping as u8 {

            state.store(WorkerState::Busy as u8, Ordering::Release);

            while executor.has_ready_tasks() {

                executor.run_ready_tasks();

                match rx.try_recv() {
                    Ok(message) => {
                        if !Self::process_message(
                            message, 
                            state.clone(), 
                            &mut executor
                        ) { return; }
                    },
                    Err(TryRecvError::Empty) => {},
                    Err(e) => {
                        error!("Worker {id} encountered an error while receiving a message: {e}");
                        state.store(WorkerState::Stopping as u8, Ordering::Release);
                        return;
                    },
                }

                if state.load(Ordering::Acquire) == WorkerState::Stopping as u8 {
                    break;
                }
            }

            if state.load(Ordering::Acquire) == WorkerState::Stopping as u8 {
                break;
            }

            state.store(WorkerState::Idle as u8, Ordering::Release);
            match rx.recv() {
                Ok(message) => {
                    if !Self::process_message(
                        message, 
                        state.clone(), 
                        &mut executor
                    ) { return; }
                },
                Err(e) => {
                    error!("Worker {id} encountered an error while receiving a message: {e}");
                    state.store(WorkerState::Stopping as u8, Ordering::Release);
                }
            }
        }

        // Allow the executor to finish any remaining tasks
        while executor.has_tasks() {
            match rx.try_recv() {
                Ok(Message::Kill) => return,
                Ok(_) => (),
                Err(TryRecvError::Empty) => (),
                Err(_) => return,
            }
            executor.run_ready_tasks();
        }
    }
}