use std::{sync::{atomic::{AtomicU8, Ordering}, Arc}, thread::JoinHandle};
use crossbeam_channel::{Receiver, SendError, Sender};
use log::{debug, error, info};
use parking_lot::Mutex;
use thiserror::Error;
use crate::{event_callback_registry::EventCallbackRegistry, event_channel::{EventChannel, EventChannelError, EventSender}, event_poller::{EventPoller, EventPollerError, EventRegistry, InterestType, Key, PollEvent}, executor::{Executor, ExecutorError}, message::Message, task::Task, worker_state::WorkerState};

static EXECUTOR_CAPACITY: usize = 8192;
static POLLER_BUFFER_SIZE: usize = 1024;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Worker {0} is already running")]
    AlreadyRunning(usize),

    #[error("Worker {0} is not running")]
    NotRunning(usize),

    #[error("A threading error occurred in worker {0}")]
    ThreadError(usize),

    #[error("The event poller encountered an error ({0})")]
    EventPollerError(#[from] EventPollerError),

    #[error("Event channel error: {0}")]
    EventChannelError(#[from] EventChannelError),

    #[error("Error when sending message: {0}")]
    MessageSendError(#[from] SendError<Message>),

    #[error("Failed to acquire data for creating worker thread")]
    TempDataUnavailable,

    #[error("Executor error: {0}")]
    Executor(#[from] ExecutorError),
}

#[derive(Debug, Clone)]
pub struct WorkSender {
    sender: Sender<Message>,
    message_signaler: EventSender,
}

impl WorkSender {
    fn new(sender: Sender<Message>, message_signaler: EventSender) -> Self {
        Self { sender, message_signaler }
    }

    pub fn send_message(&self, msg: Message) -> Result<(), WorkerError> {
        self.sender.send(msg)?;
        self.message_signaler.signal()?;
        Ok(())
    }
}

struct WorkerTempData {
    receiver: Receiver<Message>,
    poller: EventPoller,
    message_channel_key: Key,
}

pub struct Worker {
    pub id: usize,
    state: Arc<AtomicU8>,
    handle: Option<JoinHandle<()>>,
    sender: WorkSender,
    executor: Arc<Mutex<Executor>>,
    event_registry: EventRegistry,
    callback_registry: Arc<EventCallbackRegistry>,
    temp_data: Option<WorkerTempData>,
}

impl Worker {

    pub fn new(id: usize) -> Result<Self, WorkerError> {

        let (tx, rx) = crossbeam_channel::unbounded();
        let message_channel = EventChannel::new()?;
        let sender = WorkSender::new(tx, message_channel.as_sender());
        let poller = EventPoller::new(POLLER_BUFFER_SIZE)?;
        let event_registry = poller.registry();
        let message_channel_key = event_registry.register_interest(
            &message_channel,
            InterestType::READ
        )?;
        let executor = Executor::new(EXECUTOR_CAPACITY, sender.clone());

        Ok(Self {
            id,
            state: Arc::new(AtomicU8::new(WorkerState::None as u8)),
            handle: None,
            sender,
            executor: Arc::new(Mutex::new(executor)),
            event_registry,
            callback_registry: Arc::new(EventCallbackRegistry::new()),
            temp_data: Some(WorkerTempData {
                receiver: rx,
                poller,
                message_channel_key,
            })
        })
    }

    pub fn event_registry(&self) -> &EventRegistry { &self.event_registry }
    pub fn callback_registry(&self) -> &EventCallbackRegistry { &self.callback_registry }
    pub fn executor(&self) -> &Mutex<Executor> { &self.executor }

    pub fn create_sender(&self) -> WorkSender { self.sender.clone() }

    pub fn start(&mut self) -> Result<(), WorkerError> {
        if self.state.compare_exchange(
            WorkerState::None as u8,
            WorkerState::Starting as u8,
            Ordering::SeqCst,
            Ordering::SeqCst
        ).is_err() { return Err(WorkerError::AlreadyRunning(self.id)) }

        let state = Arc::clone(&self.state);
        let executor = Arc::clone(&self.executor);
        let id = self.id;
        let callback_registry = Arc::clone(&self.callback_registry);

        let temp_data = match self.temp_data.take() {
            Some(temp_data) => temp_data,
            None => return Err(WorkerError::TempDataUnavailable),
        };

        self.handle = Some(std::thread::spawn(move || {
            info!("cl-async: Worker {} starting", id);
            if let Err(e) = Self::worker_loop(
                state,
                temp_data.poller,
                executor,
                temp_data.receiver,
                callback_registry,
                temp_data.message_channel_key
            ) { error!("Worker {}: {}", id, e) }
            info!("cl-async: Worker {} stopped", id);
        }));

        Ok(())
    }

    fn set_state(state_u8: &AtomicU8, state: WorkerState) {
        state_u8.store(state as u8, Ordering::Release);
    }

    fn _get_state(state_u8: &AtomicU8) -> WorkerState {
        state_u8.load(Ordering::Acquire).into()
    }

    pub fn get_state(&self) -> WorkerState {
        Self::_get_state(&self.state)
    }

    fn _is_state(state_u8: &AtomicU8, state: WorkerState) -> bool {
        state_u8.load(Ordering::Acquire) == state as u8
    }

    pub fn is_state(&self, state: WorkerState) -> bool {
        Self::_is_state(&self.state, state)
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_some() && !(
            self.is_state(WorkerState::None) ||
            self.is_state(WorkerState::Stopped)
        )
    }

    pub fn kill(&mut self) -> Result<(), WorkerError> {
        if !self.is_running() { return Err(WorkerError::NotRunning(self.id)) }
        self.sender.send_message(Message::Kill)?;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), WorkerError> {
        if !self.is_running() { return Err(WorkerError::NotRunning(self.id)) }
        self.sender.send_message(Message::Shutdown)?;
        Ok(())
    }

    pub fn spawn(&self, task: Task) -> Result<(), WorkerError> {
        self.sender.send_message(Message::NewTask(task))
    }

    pub fn join(&mut self) -> Result<(), WorkerError> {
        let handle_opt = self.handle.take();
        match handle_opt {
            Some(handle) => match handle.join() {
                Ok(_) => Ok(()),
                Err(_) => Err(WorkerError::ThreadError(self.id)),
            },
            None => Err(WorkerError::NotRunning(self.id)),
        }
    }

    pub fn take_join_handle(&mut self) -> Option<JoinHandle<()>> {
        self.handle.take()
    }

    fn worker_loop(
        state: Arc<AtomicU8>,
        mut poller: EventPoller,
        executor: Arc<Mutex<Executor>>,
        rx: Receiver<Message>,
        callback_registry: Arc<EventCallbackRegistry>,
        message_channel_key: Key,
    ) -> Result<(), WorkerError> {

        let mut events: Vec<PollEvent> = Vec::with_capacity(1024);
        let mut tasks: Vec<Task> = Vec::new();
        let mut should_recv: bool;
        let registry = poller.registry();

        loop {

            should_recv = false;
            tasks.clear();

            Self::set_state(&state, WorkerState::Idle);
            if let Err(e) = poller.poll_events(
                &mut events, 
                None
            ) { return Err(WorkerError::EventPollerError(e)); }

            Self::set_state(&state, WorkerState::Busy);

            for event in events.iter() {
                let key = event.key;

                if key == message_channel_key { 
                    should_recv = true;
                    continue; 
                }
                
                if let Some(task) = callback_registry.try_run(
                    key, 
                    *event,
                    registry.clone()
                ) { tasks.push(task); }

            }

            events.clear();

            if should_recv {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        Message::NewTask(task) => tasks.push(task),
                        Message::Continue => (),
                        Message::Kill => {
                            Self::set_state(&state, WorkerState::Stopped);
                            return Ok(());
                        },
                        Message::Shutdown => {
                            Self::set_state(&state, WorkerState::Stopping);
                            break;
                        }
                    }
                }
            }

            if Self::_is_state(&state, WorkerState::Stopping) {
                break;
            }

            let mut lock = executor.lock();

            for task in tasks.drain(..) { lock.spawn(task)? }

            while lock.has_ready_tasks() { lock.run_ready_tasks(); }
        }

        // finish any remaining tasks
        let mut lock = executor.lock();

        while lock.has_tasks() { 
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Message::NewTask(task) => { lock.spawn(task)?; },
                    Message::Kill => {
                        Self::set_state(&state, WorkerState::Stopped);
                        return Ok(());
                    },
                    _ => (),
                }
            };
            lock.run_ready_tasks(); 
        }
        
        Self::set_state(&state, WorkerState::Stopped);

        Ok(())
    }
}