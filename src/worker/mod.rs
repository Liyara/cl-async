pub mod message;
pub mod state;
pub mod work_sender;

pub use message::Message;
pub use state::WorkerState;
pub use work_sender::WorkSender;

use thiserror::Error;
use parking_lot::Mutex;
use work_sender::WorkSenderError;

use std::{
    collections::VecDeque, sync::{
        atomic::{
            AtomicU8, 
            Ordering
        }, 
        Arc
    }, 
    thread::JoinHandle
};

use crate::{
    events::{
        channel::{
            EventChannelError, 
            EventChannelReceiver
        }, poller::{
            registry::EventPollerRegistryError, 
            EventPollerError, 
            EventPollerRegistry, 
            InterestType
        }, 
        Event, 
        EventChannel, 
        EventPoller, 
        EventQueueRegistry, 
        EventSource
    }, io::{
        context::IOContextError, 
        submission::IOSubmissionError, 
        IOCompletion, 
        IOCompletionQueue, 
        IOContext, 
        IOOperation, 
        IOSubmissionQueue
    }, 
    task::{
        executor::ExecutorError, 
        Executor
    },  
    Key, 
    OSError, 
    Task
};

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

    #[error("Event Poller Registry error: {0}")]
    EventPollerRegistryError(#[from] EventPollerRegistryError),

    #[error("Event Channel error: {0}")]
    EventChannelError(#[from] EventChannelError),

    #[error("IO Context error: {0}")]
    IOContextError(#[from] IOContextError),

    #[error("IO Submission Queue error: {0}")]
    IOSubmissionQueueError(#[from] IOSubmissionError),

    #[error("Work Sender error: {0}")]
    WorkSenderError(#[from] WorkSenderError),

    #[error("Failed to acquire data for creating worker thread")]
    TempDataUnavailable,

    #[error("Executor error: {0}")]
    ExecutorError(#[from] ExecutorError)
}

pub struct WorkerIOSubmissionHandle {
    key: Key,
    completion_queue: IOCompletionQueue,
}

impl WorkerIOSubmissionHandle {
    fn new(key: Key, completion_queue: IOCompletionQueue) -> Self {
        Self { key, completion_queue }
    }

    pub fn is_completed(&self) -> bool {
        self.completion_queue.peek(self.key)
    }

    pub fn complete(self) -> Option<Result<IOCompletion, i32>> {
        self.completion_queue.pop(self.key)
    }
}

pub struct WorkerMultipleIOSubmissionHandle {
    keys: Vec<Key>,
    completed_keys: usize,
    completion_queue: IOCompletionQueue,
}

impl WorkerMultipleIOSubmissionHandle {
    fn new(keys: Vec<Key>, completion_queue: IOCompletionQueue) -> Self {
        Self { keys, completed_keys: 0, completion_queue, }
    }

    pub fn is_completed(&self) -> bool {
        self.completed_keys == self.keys.len()
    }

    pub fn complete(&mut self) -> Vec<(Result<IOCompletion, i32>, usize)> {
        let mut completed_keys = Vec::new();
        let mut i  = 0;
        for key in &self.keys {
            match self.completion_queue.pop(*key) {
                Some(val) => {
                    completed_keys.push((val, i));
                    self.completed_keys += 1;
                }
                None => continue,
            };
            i += 1;
        }
        completed_keys
    }
}

struct WorkerTempData {
    receiver: crossbeam_channel::Receiver<Message>,
    poller: EventPoller,
    executor: Executor,
    io_context: IOContext,
    message_channel_key_and_receiver: (Key, EventChannelReceiver),
    io_channel_key_and_receiver: (Key, EventChannelReceiver),
}

pub struct Worker {
    id: usize,
    state: Arc<AtomicU8>,
    handle: Option<JoinHandle<()>>,
    sender: WorkSender,
    event_registry: EventPollerRegistry,
    queue_registry: EventQueueRegistry,
    io_completion_queue: IOCompletionQueue,
    io_submission_queue: IOSubmissionQueue,
    temp_data: Mutex<Option<WorkerTempData>>,
}

impl Worker {

    pub fn new(id: usize) -> Result<Self, WorkerError> {

        let (tx, rx) = crossbeam_channel::unbounded();
        let message_channel = EventChannel::new()?;
        let sender = WorkSender::new(tx, message_channel.as_sender());
        let poller = EventPoller::new(POLLER_BUFFER_SIZE)?;
        let event_registry = poller.registry();
        let message_channel_key: Key = event_registry.register_interest(
            EventSource::new(&message_channel),
            InterestType::READ
        )?;
        let executor = Executor::new(sender.clone());
        
        let mut io_context = IOContext::new(256)?;
        let io_channel = EventChannel::new()?;
        io_context.register_event_channel(&io_channel)?;
        let io_channel_key = event_registry.register_interest(
            EventSource::new(&io_channel),
            InterestType::READ
        )?;

        let io_submission_queue = IOSubmissionQueue::new(
            sender.clone()
        );

        let queue_registry = EventQueueRegistry::new();
        

        Ok(Self {
            id,
            state: Arc::new(AtomicU8::new(WorkerState::None as u8)),
            handle: None,
            sender,
            event_registry,
            queue_registry,
            io_completion_queue: io_context.completion().clone(),
            io_submission_queue,
            temp_data: Mutex::new(Some(WorkerTempData {
                receiver: rx,
                poller,
                io_context,
                executor,
                message_channel_key_and_receiver: (
                    message_channel_key,
                    message_channel.as_receiver()
                ),
                io_channel_key_and_receiver: (
                    io_channel_key,
                    io_channel.as_receiver()
                )
            }))
        })
    }

    pub fn event_registry(&self) -> &EventPollerRegistry { &self.event_registry }
    pub fn queue_registry(&self) -> &EventQueueRegistry { &self.queue_registry }
    pub fn id(&self) -> usize { self.id }

    pub fn submit_io_operations(
        &self,
        operations: Vec<IOOperation>, 
        waker: Option<std::task::Waker>
    ) -> Result<WorkerMultipleIOSubmissionHandle, WorkerError> {
        let keys = self.io_submission_queue.submit_multiple(operations, waker)?;
        Ok(WorkerMultipleIOSubmissionHandle::new(keys, self.io_completion_queue.clone()))
    }

    pub fn submit_io_operation(
        &self,
        operation: IOOperation, 
        waker: Option<std::task::Waker>
    ) -> Result<WorkerIOSubmissionHandle, WorkerError> {
        let key = self.io_submission_queue.submit(operation, waker)?;
        Ok(WorkerIOSubmissionHandle::new(key, self.io_completion_queue.clone()))
    }

    pub fn start(&mut self) -> Result<(), WorkerError> {
        if self.state.compare_exchange(
            WorkerState::None as u8,
            WorkerState::Starting as u8,
            Ordering::SeqCst,
            Ordering::SeqCst
        ).is_err() { return Err(WorkerError::AlreadyRunning(self.id)) }

        let state = Arc::clone(&self.state);
        let id = self.id;
        let queue_registry = self.queue_registry.clone();
        let temp_data = match self.temp_data.lock().take() {
            Some(temp_data) => temp_data,
            None => return Err(WorkerError::TempDataUnavailable),
        };

        self.handle = Some(std::thread::spawn(move || {
            info!("cl-async: Worker {} starting", id);
            if let Err(e) = Self::worker_loop(
                id,
                Arc::clone(&state),
                temp_data.poller,
                temp_data.executor,
                temp_data.receiver,
                queue_registry,
                temp_data.io_context,
                temp_data.message_channel_key_and_receiver,
                temp_data.io_channel_key_and_receiver
            ) { error!("cl-async: Worker {}: {}", id, e) }
            Self::set_state(&state, WorkerState::Stopped);
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

    pub fn kill(&self) -> Result<(), WorkerError> {
        if !self.is_running() { return Err(WorkerError::NotRunning(self.id)) }
        self.sender.send_message(Message::Kill)?;
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), WorkerError> {
        if !self.is_running() { return Err(WorkerError::NotRunning(self.id)) }
        self.sender.send_message(Message::Shutdown)?;
        Ok(())
    }

    pub fn spawn(&self, task: Task) -> Result<(), WorkerError> {
        Ok(self.sender.send_message(Message::SpawnTask(task))?)
    }

    pub fn spawn_multiple(&self, tasks: Vec<Task>) -> Result<(), WorkerError> {
        Ok(self.sender.send_message(Message::SpawnTasks(tasks))?)
    }

    pub fn take_join_handle(&mut self) -> Option<JoinHandle<()>> {
        self.handle.take()
    }

    fn worker_loop(
        id: usize,
        state: Arc<AtomicU8>,
        mut poller: EventPoller,
        mut executor: Executor,
        rx: crossbeam_channel::Receiver<Message>,
        queue_registry: EventQueueRegistry,
        mut io_context: IOContext,
        message_channel_key_and_receiver: (Key, EventChannelReceiver),
        io_channel_key_and_receiver: (Key, EventChannelReceiver),
    ) -> Result<(), WorkerError> {

        let mut events: Vec<Event> = Vec::with_capacity(1024);
        let mut queues_to_wake = VecDeque::new();
        let mut should_recv: bool;
        let mut should_complete_io: bool;
        let mut should_submit_io: bool;

        let (message_channel_key, message_channel_receiver) = message_channel_key_and_receiver;
        let (io_channel_key, io_channel_receiver) = io_channel_key_and_receiver;

        loop {

            should_recv = false;
            should_complete_io = false;
            should_submit_io = false;

            Self::set_state(&state, WorkerState::Idle);
            match poller.poll_events(
                &mut events, 
                None
            ) { 
                Ok(()) => {},
                Err(EventPollerError::FailedToPollEvents { source }) => {
                    match source {
                        OSError::OperationInterrupted => {
                            continue;
                        },
                        _ => {
                            return Err(
                                WorkerError::EventPollerError(
                                    EventPollerError::FailedToPollEvents { source }
                                )
                            );
                        }
                    }
                }
                Err(e) => return Err(WorkerError::EventPollerError(e))
            }

            Self::set_state(&state, WorkerState::Busy);

            for event in events.iter() {
                let key = event.key;

                if key == message_channel_key { 
                    should_recv = true;
                    continue; 
                }

                if key == io_channel_key {
                    should_complete_io = true;
                    continue;
                }
                
                if queue_registry.push_event(key, *event) {
                    queues_to_wake.push_back(key);
                }

            }

            while let Some(key) = queues_to_wake.pop_front() {
                queue_registry.wake(key);
            }

            events.clear();

            if should_complete_io {
                io_context.complete();
                io_channel_receiver.drain()?;
            }

            if should_recv {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        Message::SpawnTask(task) => {
                            executor.spawn(task);
                        }
                        Message::SpawnTasks(tasks) => {
                            for task in tasks { executor.spawn(task) }
                        }
                        Message::WakeTask(task_id) => executor.wake_task(task_id),
                        Message::SubmitIOEntry(io_entry) => {
                            io_context.prepare_submission(io_entry)?;
                            should_submit_io = true;
                        }
                        Message::SubmitIOEntries(io_entries) => {
                            for entry in io_entries {
                                io_context.prepare_submission(entry)?;
                            }
                            should_submit_io = true;
                        },
                        Message::Kill => { return Ok(()); },
                        Message::Shutdown => {
                            Self::set_state(&state, WorkerState::Stopping);
                        }
                        _ => ()
                    }
                }
                message_channel_receiver.drain()?;
            }

            if should_submit_io { io_context.submit()?; }

            if Self::_is_state(&state, WorkerState::Stopping) {
                break;
            }

            while executor.has_ready_tasks() { executor.run_ready_tasks(); }
        }

        info!("cl-async: Worker {} shutting down...", id);

        io_channel_receiver.drain()?;
        message_channel_receiver.drain()?;

        loop {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Message::SpawnTask(task) => {
                        executor.spawn(task);
                    }
                    Message::SpawnTasks(tasks) => {
                        for task in tasks { executor.spawn(task) }
                    }
                    Message::WakeTask(task_id) => executor.wake_task(task_id),
                    Message::SubmitIOEntry(io_entry) => {
                        io_context.prepare_submission(io_entry)?;
                    }
                    Message::SubmitIOEntries(io_entries) => {
                        for entry in io_entries {
                            io_context.prepare_submission(entry)?;
                        }
                    },
                    Message::Kill => { return Ok(()); },
                    _ => ()
                }
            };

            io_context.blocking_submit()?;
            io_context.complete();

            while executor.has_ready_tasks() { executor.run_ready_tasks(); }
            if !executor.has_running_tasks() { break; }
        }

        Ok(())
    }
}