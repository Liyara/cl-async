use std::{sync::{atomic::{AtomicU8, Ordering}, Arc}, thread::JoinHandle};
use crossbeam_channel::{Receiver, SendError, Sender};
use log::{error, info};
use parking_lot::Mutex;
use thiserror::Error;
use crate::{event_callback_registry::{EventCallbackRegistry, EventCallbackRegistryError}, event_channel::{EventChannel, EventChannelError, EventSender}, event_poller::{EventPoller, EventPollerError, EventRegistry, InterestType, Key, PollEvent}, executor::{Executor, ExecutorError}, message::Message, task::Task, worker_state::WorkerState};

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

    #[error("Event callback registry error: {0}")]
    EventCallbackRegistry(#[from] EventCallbackRegistryError),
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

#[derive(Debug, Clone)]
pub struct WorkStealer {
    inner: crossbeam_deque::Stealer<Task>,
    sender: WorkSender,
}

impl WorkStealer {
    fn new(stealer: crossbeam_deque::Stealer<Task>, sender: WorkSender) -> Self {
        Self { inner: stealer, sender }
    }

    fn _check_steal(&self, steal: crossbeam_deque::Steal<Task>) -> crossbeam_deque::Steal<Task> {
        match steal {
            crossbeam_deque::Steal::Success(task) => {
                if let Err(e) = self.sender.send_message(Message::Stolen(vec![task.id])) {
                    error!("Failed to send stolen task to worker: {e}");
                    if let Err(e) = self.sender.send_message(Message::NewTask(task)) {
                        error!("Failed to send new task to worker: {e}");
                    }
                    crossbeam_deque::Steal::<Task>::Retry
                } else { crossbeam_deque::Steal::Success(task) }
            },
            _ => steal
        }
    }

    fn _steal(&self) -> crossbeam_deque::Steal<Task> {
        let steal = self.inner.steal();
        self._check_steal(steal)
    }

    pub fn steal_batch(&self) -> crossbeam_deque::Steal<Vec<Task>> {
        let dest: crossbeam_deque::Worker<Task> = crossbeam_deque::Worker::new_fifo();
        match self.inner.steal_batch(&dest) {
            crossbeam_deque::Steal::Success(()) => {
                let mut r: Vec<Task> = Vec::with_capacity(dest.len());
                while let Some(task) = dest.pop() { r.push(task); }
                if let Err(e) = self.sender.send_message(Message::Stolen(r.iter().map(|t| t.id).collect())) {
                    error!("Failed to send stolen tasks to worker: {e}");
                    for task in r {
                        if let Err(e) = self.sender.send_message(Message::NewTask(task)) {
                            error!("Failed to send new task to worker: {e}");
                        }
                    }
                    crossbeam_deque::Steal::Retry
                } else { crossbeam_deque::Steal::Success(r) }
            },
            crossbeam_deque::Steal::Retry => crossbeam_deque::Steal::Retry,
            crossbeam_deque::Steal::Empty => crossbeam_deque::Steal::Empty,
        }
    }
}

struct WorkerTempData {
    receiver: Receiver<Message>,
    poller: EventPoller,
    executor: Executor,
    message_channel_key: Key,
}

pub struct Worker {
    id: usize,
    state: Arc<AtomicU8>,
    handle: Option<JoinHandle<()>>,
    sender: WorkSender,
    stealer: WorkStealer,
    event_registry: EventRegistry,
    callback_registry: Arc<EventCallbackRegistry>,
    temp_data: Mutex<Option<WorkerTempData>>,
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
        let executor = Executor::new(sender.clone());
        let stealer = WorkStealer::new(executor.stealer(), sender.clone());


        Ok(Self {
            id,
            state: Arc::new(AtomicU8::new(WorkerState::None as u8)),
            handle: None,
            sender,
            stealer,
            event_registry,
            callback_registry: Arc::new(EventCallbackRegistry::new()),
            temp_data: Mutex::new(Some(WorkerTempData {
                receiver: rx,
                poller,
                executor,
                message_channel_key,
            }))
        })
    }

    pub fn event_registry(&self) -> &EventRegistry { &self.event_registry }
    pub fn callback_registry(&self) -> &EventCallbackRegistry { &self.callback_registry }
    pub fn stealer(&self) -> WorkStealer { self.stealer.clone() }

    pub fn start(&mut self) -> Result<(), WorkerError> {
        if self.state.compare_exchange(
            WorkerState::None as u8,
            WorkerState::Starting as u8,
            Ordering::SeqCst,
            Ordering::SeqCst
        ).is_err() { return Err(WorkerError::AlreadyRunning(self.id)) }

        let state = Arc::clone(&self.state);
        let id = self.id;
        let callback_registry = Arc::clone(&self.callback_registry);
        let temp_data = match self.temp_data.lock().take() {
            Some(temp_data) => temp_data,
            None => return Err(WorkerError::TempDataUnavailable),
        };

        self.handle = Some(std::thread::spawn(move || {
            info!("cl-async: Worker {} starting", id);
            if let Err(e) = Self::worker_loop(
                id,
                state,
                temp_data.poller,
                temp_data.executor,
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

    pub fn take_join_handle(&mut self) -> Option<JoinHandle<()>> {
        self.handle.take()
    }

    fn worker_loop(
        id: usize,
        state: Arc<AtomicU8>,
        mut poller: EventPoller,
        mut executor: Executor,
        rx: Receiver<Message>,
        callback_registry: Arc<EventCallbackRegistry>,
        message_channel_key: Key,
    ) -> Result<(), WorkerError> {

        let mut events: Vec<PollEvent> = Vec::with_capacity(1024);
        let mut should_recv: bool;
        let registry = poller.registry();

        loop {

            should_recv = false;

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
                
                if let Some(task) = callback_registry.run(
                    key, 
                    *event,
                    registry.clone()
                )? { executor.spawn(task); }

            }

            events.clear();

            if should_recv {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        Message::NewTask(task) => executor.spawn(task),
                        Message::WakeTask(task_id) => executor.wake_task(task_id)?,
                        Message::Stolen(task_ids) => {
                            for task_id in task_ids { 
                                executor.clear_cache_for(task_id)?;
                            }
                        }
                        Message::Kill => {
                            Self::set_state(&state, WorkerState::Stopped);
                            return Ok(());
                        },
                        Message::Shutdown => {
                            Self::set_state(&state, WorkerState::Stopping);
                            break;
                        }
                        _ => ()
                    }
                }
            }

            if Self::_is_state(&state, WorkerState::Stopping) {
                break;
            }

            while executor.has_ready_tasks() { executor.run_ready_tasks()?; }

            if !executor.has_running_tasks() {
                // we ran out of work, steal some if we can
                let stealers = crate::get_stealers_rand(id);
                for stealer in stealers.iter() {
                    let steal = stealer.steal_batch();
                    if let crossbeam_deque::Steal::Success(batch) = steal {
                        if batch.is_empty() { continue; }
                        for task in batch {
                            executor.spawn(task);
                        }
                        break;
                    }
                }
            }
        }

        // finish any remaining tasks

        while executor.has_ready_tasks() || executor.has_running_tasks() { 
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Message::NewTask(task) => executor.spawn(task),
                    Message::WakeTask(task_id) => executor.wake_task(task_id)?,
                    Message::Stolen(task_ids) => {
                        for task_id in task_ids { 
                            executor.clear_cache_for(task_id)?;
                        }
                    },
                    Message::Kill => {
                        Self::set_state(&state, WorkerState::Stopped);
                        return Ok(());
                    },
                    _ => (),
                }
            };
            executor.run_ready_tasks()?; 
        }
        
        Self::set_state(&state, WorkerState::Stopped);

        Ok(())
    }
}