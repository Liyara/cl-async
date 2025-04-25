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
    collections::VecDeque, os::fd::RawFd, sync::{
        atomic::{
            AtomicU8, 
            Ordering
        }, 
        Arc
    }, task::Poll, thread::JoinHandle
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
        EventSource, 
        EventType
    }, io::{IoCompletionQueue, IoCompletionResult, IoContext, IoError, IoOperation, IoSubmission, IoSubmissionQueue}, notifications::{self, Notification, NotificationFlags, Signal}, task::{
        executor::ExecutorError, 
        Executor
    }, Key, OsError, Task
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

    #[error("Work Sender error: {0}")]
    WorkSenderError(#[from] WorkSenderError),

    #[error("Worker IO error: {0}")]
    IoError(#[from] IoError),

    #[error("Worker OS error: {0}")]
    OsError(#[from] OsError),

    #[error("Failed to acquire data for creating worker thread")]
    TempDataUnavailable,

    #[error("Failed to broadcast notification")]
    NotificationError(#[from] async_broadcast::SendError<Notification>),

    #[error("Executor error: {0}")]
    ExecutorError(#[from] ExecutorError)
}

pub struct WorkerIOSubmissionHandle {
    pub key: Key,
    pub completion_queue: IoCompletionQueue,
}

impl WorkerIOSubmissionHandle {
    
    fn new(key: Key, completion_queue: IoCompletionQueue) -> Self {
        Self { key, completion_queue }
    }

    pub fn poll(
        &self, 
        cx: &mut std::task::Context<'_>
    ) -> Poll<Option<IoCompletionResult>> {
        self.completion_queue.poll(self.key, cx)
    }
}

pub struct WorkerMultipleIOSubmissionHandle {
    pub keys: Vec<Key>,
    pub completion_queue: IoCompletionQueue,
}

impl WorkerMultipleIOSubmissionHandle {
    fn new(keys: Vec<Key>, completion_queue: IoCompletionQueue) -> Self {
        Self { keys, completion_queue, }
    }
}

pub struct WorkerHandle {
    pub id: usize,
    pub sender: WorkSender,
    pub event_registry: EventPollerRegistry,
    pub queue_registry: EventQueueRegistry,
    pub io_completion_queue: IoCompletionQueue,
    pub io_submission_queue: IoSubmissionQueue,
    notification_receiver: async_broadcast::Receiver<Notification>,
}

impl WorkerHandle {
    pub fn notify_on(
        &self, 
        flags: NotificationFlags
    ) -> Result<notifications::Subscription, WorkerError> {
        let subscription = notifications::Subscription::new(
            self.notification_receiver.clone(),
            flags
        );
        Ok(subscription)
    }
}

struct WorkerTempData {
    receiver: crossbeam_channel::Receiver<Message>,
    poller: EventPoller,
    executor: Executor,
    io_context: IoContext,
    message_channel_key_and_receiver: (Key, EventChannelReceiver),
    io_channel_key_and_receiver: (Key, EventChannelReceiver),
    notification_sender: async_broadcast::Sender<Notification>,
}

pub struct Worker {
    id: usize,
    state: Arc<AtomicU8>,
    handle: Option<JoinHandle<()>>,
    sender: WorkSender,
    event_registry: EventPollerRegistry,
    queue_registry: EventQueueRegistry,
    io_completion_queue: IoCompletionQueue,
    io_submission_queue: IoSubmissionQueue,
    notification_receiver: async_broadcast::Receiver<Notification>,
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
        
        let mut io_context = IoContext::new(256)?;
        let io_channel = EventChannel::new()?;
        io_context.register_event_channel(&io_channel)?;
        let io_channel_key = event_registry.register_interest(
            EventSource::new(&io_channel),
            InterestType::READ
        )?;

        let io_submission_queue = IoSubmissionQueue::new(
            sender.clone()
        );

        let queue_registry = EventQueueRegistry::new();
        
        let (
            notification_sender, 
            notification_receiver
        ) = async_broadcast::broadcast(128);

        Ok(Self {
            id,
            state: Arc::new(AtomicU8::new(WorkerState::None as u8)),
            handle: None,
            sender,
            event_registry,
            queue_registry,
            io_completion_queue: io_context.completion().clone(),
            io_submission_queue,
            notification_receiver,
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
                ),
                notification_sender,
            }))
        })
    }

    pub fn event_registry(&self) -> &EventPollerRegistry { &self.event_registry }
    pub fn queue_registry(&self) -> &EventQueueRegistry { &self.queue_registry }
    pub fn id(&self) -> usize { self.id }

    fn _submit_io_operations(
        &self,
        operations: Vec<IoOperation>, 
        waker: Option<std::task::Waker>
    ) -> Result<Vec<Key>, IoError> {
        let keys = self.io_submission_queue.submit_multiple(operations, waker)?;
        Ok(keys)
    }

    pub fn submit_io_operations(
        &self,
        operations: Vec<IoOperation>, 
        waker: Option<std::task::Waker>
    ) -> Result<WorkerMultipleIOSubmissionHandle, WorkerError> {
        let keys = self._submit_io_operations(operations, waker)?;
        Ok(WorkerMultipleIOSubmissionHandle::new(keys, self.io_completion_queue.clone()))
    }

    fn _submit_io_operation(
        &self,
        operation: IoOperation, 
        waker: Option<std::task::Waker>
    ) -> Result<Key, IoError> {
        let key = self.io_submission_queue.submit(operation, waker)?;
        Ok(key)
    }

    pub fn submit_io_operation(
        &self,
        operation: IoOperation, 
        waker: Option<std::task::Waker>
    ) -> Result<WorkerIOSubmissionHandle, WorkerError> {
        let key = self._submit_io_operation(operation, waker)?;
        Ok(WorkerIOSubmissionHandle::new(key, self.io_completion_queue.clone()))
    }

    pub fn as_handle(&self) -> WorkerHandle {
        WorkerHandle {
            id: self.id,
            sender: self.sender.clone(),
            event_registry: self.event_registry.clone(),
            queue_registry: self.queue_registry.clone(),
            io_completion_queue: self.io_completion_queue.clone(),
            io_submission_queue: self.io_submission_queue.clone(),
            notification_receiver: self.notification_receiver.clone(),
        }
    }

    pub fn notify_on(
        &self, 
        flags: NotificationFlags
    ) -> Result<notifications::Subscription, WorkerError> {
        let subscription = notifications::Subscription::new(
            self.notification_receiver.clone(),
            flags
        );
        Ok(subscription)
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
                queue_registry.clone(),
                temp_data.io_context,
                temp_data.message_channel_key_and_receiver,
                temp_data.io_channel_key_and_receiver,
                temp_data.notification_sender.clone()
            ) { error!("cl-async: Worker {}: {}", id, e) }
            if !Self::_is_state(&state, WorkerState::Stopping) {
                // Shutdown is not graceful.
                if let Err(e) = Self::broadcast_notification(
                    &temp_data.notification_sender, 
                    Notification::Kill
                ) { error!("cl-async: Worker {}: {}", id, e); }
                queue_registry.broadcast_and_wake(EventType::KILL);
            }
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

    unsafe fn block_async_signals() -> Result<RawFd, OsError> {
        let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        
        syscall!(sigemptyset(&mut mask))?;

        for signal in Signal::handleable() {
            syscall!(sigaddset(&mut mask, signal.into()))?;
        }

        syscall!(sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()))?;

        let sfd = syscall!(signalfd(
            -1,
            &mask,
            libc::SFD_NONBLOCK | libc::SFD_CLOEXEC
        ))?;

        Ok(sfd)
    }

    fn broadcast_notification(
        notification_sender: &async_broadcast::Sender<Notification>,
        notification: Notification
    ) -> Result<(), WorkerError> {
        notification_sender.broadcast_blocking(notification)?;
        Ok(())
    }

    fn prepare_io_submission(
        io_context: &mut IoContext,
        submission: IoSubmission,
        backup_queue: &mut VecDeque<IoSubmission>
    ) -> bool {
        if let Some(submission) = io_context.prepare_submission(submission) {
            backup_queue.push_back(submission);
            false
        } else { true }
    }

    fn worker_loop(
        id: usize,
        state: Arc<AtomicU8>,
        mut poller: EventPoller,
        mut executor: Executor,
        rx: crossbeam_channel::Receiver<Message>,
        queue_registry: EventQueueRegistry,
        mut io_context: IoContext,
        message_channel_key_and_receiver: (Key, EventChannelReceiver),
        io_channel_key_and_receiver: (Key, EventChannelReceiver),
        notification_sender: async_broadcast::Sender<Notification>
    ) -> Result<(), WorkerError> {

        let mut events: Vec<Event> = Vec::with_capacity(1024);
        let mut queues_to_wake = VecDeque::new();
        let mut should_recv: bool;
        let mut should_complete_io: bool;
        let mut should_submit_io: bool;

        let (message_channel_key, message_channel_receiver) = message_channel_key_and_receiver;
        let (io_channel_key, io_channel_receiver) = io_channel_key_and_receiver;

        let sfd = unsafe { Self::block_async_signals()? };

        let sfd_key = poller.registry().register_interest(
            EventSource::new(&sfd),
            InterestType::READ
        )?;

        let mut waiting_submissions: VecDeque<IoSubmission> = VecDeque::new();

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
                        OsError::OperationInterrupted => {
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

                if key == sfd_key {
                    let mut siginfo: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };
                    
                    let ret = syscall!(
                        read(sfd, 
                            &mut siginfo as *mut _ as *mut libc::c_void, 
                            std::mem::size_of::<libc::signalfd_siginfo>()
                        )
                    ).map_err(|e| {
                        WorkerError::OsError(OsError::from(e))
                    })?;

                    if ret != std::mem::size_of::<libc::signalfd_siginfo>() as isize {
                        warn!("cl-async: Worker {}: Failed to read signal info", id);
                        continue;
                    }

                    let signal = Signal::from(siginfo.ssi_signo as libc::c_int);

                    let msg = match signal {
                        Signal::Continue => Some(Notification::Continue),
                        Signal::TtyIn | Signal::TtyOut => None,
                        
                        Signal::Interrupt
                        | Signal::Terminate
                        | Signal::Hangup
                        | Signal::Quit
                        | Signal::Alarm => {
                            Some(Notification::Shutdown)
                        },

                        Signal::UserDefined(code) => {
                            Some(Notification::Custom(code as u64))
                        },

                        Signal::ChildStopped => {
                            Some(Notification::ChildStopped)
                        },

                        _ => {
                            warn!("cl-async: Worker {}: Received signal: {}", id, signal);
                            None
                        }
                    };

                    if let Some(msg) = msg {
                        match msg {
                            Notification::Shutdown => {
                                Self::set_state(&state, WorkerState::Stopping);
                                queue_registry.broadcast_and_wake(EventType::SHUTDOWN);
                            },
                            Notification::Kill => {
                                return Ok(());
                            },
                            _ => Self::broadcast_notification(
                                &notification_sender,
                                msg
                            )?
                        }
                    }
                }
                
                if queue_registry.push_event(*event) {
                    queues_to_wake.push_back(key);
                }

            }

            while let Some(key) = queues_to_wake.pop_front() {
                queue_registry.wake(key);
            }

            events.clear();

            if should_complete_io {
                io_context.complete();

                // Push any waiting submissions to the submission queue
                while let Some(submission) = waiting_submissions.pop_front() {
                    if let Some(submission) = io_context.prepare_submission(submission) {
                        waiting_submissions.push_back(submission);
                        break;
                    } else if !should_submit_io {
                        should_submit_io = true;
                    }
                }
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
                        Message::SubmitIO(submission) => {
                            Self::prepare_io_submission(
                                &mut io_context, 
                                submission, 
                                &mut waiting_submissions
                            );
                            should_submit_io = true;
                        }
                        Message::SubmitIOMulti(submissions) => {
                            for submission in submissions {
                                Self::prepare_io_submission(
                                    &mut io_context, 
                                    submission, 
                                    &mut waiting_submissions
                                );
                            }
                            should_submit_io = true;
                        },
                        Message::Kill => { 
                            return Ok(()); 
                        },
                        Message::Shutdown => {
                            Self::set_state(&state, WorkerState::Stopping);
                        },
                        Message::Continue(waker) => {
                            waker.wake();
                        }
                    }
                }
            }

            if should_submit_io { 
                if let Err(e) = io_context.submit() {
                    error!("cl-async: Worker {}: Failed to submit IO operations: {}", id, e);
                }
            }

            while executor.has_ready_tasks() { executor.run_ready_tasks(); }

            if Self::_is_state(&state, WorkerState::Stopping) {
                queue_registry.broadcast_and_wake(EventType::SHUTDOWN);
                Self::broadcast_notification(
                    &notification_sender,
                    Notification::Shutdown
                )?;
                break;
            }
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
                    Message::SubmitIO(submission) => {
                        Self::prepare_io_submission(
                            &mut io_context, 
                            submission, 
                            &mut waiting_submissions
                        );
                    }
                    Message::SubmitIOMulti(submissions) => {
                        for submission in submissions {
                            Self::prepare_io_submission(
                                &mut io_context, 
                                submission, 
                                &mut waiting_submissions
                            );
                        }
                    },
                    Message::Kill => {
                        Self::set_state(&state, WorkerState::Stopped);
                        return Ok(()); 
                    },
                    Message::Continue(waker) => {
                        waker.wake();
                    },
                    _ => ()
                }
            };

            io_context.blocking_submit()?;
            io_context.complete();

            while executor.has_ready_tasks() { executor.run_ready_tasks(); }
            if !executor.has_running_tasks() { break; }
        }

        queue_registry.clear();

        Ok(())
    }
}