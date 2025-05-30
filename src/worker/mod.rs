pub mod message;
pub mod state;
pub mod work_sender;

pub use message::Message;
pub use state::WorkerState;
pub use work_sender::WorkSender;

use thiserror::Error;
use parking_lot::Mutex;
use work_sender::SendToWorkerChannelError;

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
    }, io::{
        IoCompletionQueue, IoCompletionResult, IoContext, IoContextError, IoOperation, IoSubmission, IoSubmissionError, IoSubmissionQueue
    }, 
    notifications::{
        self, 
        Notification, 
        NotificationFlags, 
        Signal
    }, 
    task::Executor, 
    Key, 
    OsError, 
    Task
};

static POLLER_BUFFER_SIZE: usize = 1024;

#[derive(Debug, Error)]
pub enum WorkerInitializationError {
    #[error("Failed to create event channel: {0}")]
    EventChannelError(#[from] EventChannelError),

    #[error("Failed to create event poller: {0}")]
    EventPollerError(#[from] EventPollerError),

    #[error("Failed to register message channel with poller: {0}")]
    MessageChannelPollerRegistrationError(#[source] EventPollerRegistryError),

    #[error("Failed to register IO channel with poller: {0}")]
    IoChannelPollerRegistrationError(#[source] EventPollerRegistryError),

    #[error("Failed to create IO context: {0}")]
    IoContextCreationError(#[source] IoContextError),

    #[error("Failed to register IO channel with context: {0}")]
    IoChannelContextRegistrationError(#[source] IoContextError),
}

#[derive(Debug, Error)]
pub enum WorkerStartErrorType {
    #[error("Worker is already running")]
    AlreadyRunning,

    #[error("Temp data is missing")]
    TempDataUnavailable,
}

#[derive(Debug, Error)]
#[error("Error while starting worker {id}: {source}")]
pub struct WorkerStartError {
    pub id: usize,

    #[source]
    pub source: WorkerStartErrorType,
}

impl WorkerStartError {
    pub fn new(id: usize, source: WorkerStartErrorType) -> Self {
        Self { id, source }
    }

    pub fn already_running(id: usize) -> Self {
        Self::new(id, WorkerStartErrorType::AlreadyRunning)
    }

    pub fn temp_data_unavailable(id: usize) -> Self {
        Self::new(id, WorkerStartErrorType::TempDataUnavailable)
    }
}

#[derive(Debug, Error)]
pub enum WorkerStopErrorType {
    #[error("Worker is not running")]
    NotRunning,

    #[error("Failed to send message to worker: {0}")]
    SendToWorkerChannelError(#[from] SendToWorkerChannelError),
}

#[derive(Debug, Error)]
#[error("Error while stopping worker {id}: {source}")]
pub struct WorkerStopError {
    pub id: usize,

    #[source]
    pub source: WorkerStopErrorType,
}

impl WorkerStopError {
    pub fn new(id: usize, source: WorkerStopErrorType) -> Self {
        Self { id, source }
    }

    pub fn not_running(id: usize) -> Self {
        Self::new(id, WorkerStopErrorType::NotRunning)
    }
}

#[derive(Debug, Error)]
pub enum WorkerRuntimeErrorType {
    #[error("Failed to block signals: {0}")]
    FailedToBlockSignals(#[source] OsError),

    #[error("Failed to register signal channel: {0}")]
    FailedToRegisterSignalChannel(#[source] EventPollerRegistryError),
}

#[derive(Debug, Error)]
#[error("Error while running worker {id}: {source}")]
pub struct WorkerRuntimeError {
    pub id: usize,

    #[source]
    pub source: WorkerRuntimeErrorType,
}

impl WorkerRuntimeError {
    pub fn new(id: usize, source: WorkerRuntimeErrorType) -> Self {
        Self { id, source }
    }

    pub fn failed_to_block_signals(id: usize, source: OsError) -> Self {
        Self::new(id, WorkerRuntimeErrorType::FailedToBlockSignals(source))
    }

    pub fn failed_to_register_signal_channel(id: usize, source: EventPollerRegistryError) -> Self {
        Self::new(id, WorkerRuntimeErrorType::FailedToRegisterSignalChannel(source))
    }
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Failed to create worker: {0}")]
    WorkerCreationError(#[from] WorkerInitializationError),

    #[error("Failed to start worker: {0}")]
    WorkerStartError(#[from] WorkerStartError),

    #[error("Failed to stop worker: {0}")]
    WorkerStopError(#[from] WorkerStopError),

    #[error("Failed to run worker: {0}")]
    WorkerRuntimeError(#[from] WorkerRuntimeError),
}

pub struct WorkerIoSubmissionHandle {
    pub key: Key,
    pub completion_queue: IoCompletionQueue,
}

impl WorkerIoSubmissionHandle {
    
    fn new(key: Key, completion_queue: IoCompletionQueue) -> Self {
        Self { key, completion_queue }
    }

    pub fn poll(
        &self, 
        cx: &mut std::task::Context<'_>
    ) -> Poll<Option<IoCompletionResult>> {
        self.completion_queue.poll(self.key, cx)
    }

    pub fn cancel(
        self
    ) -> Result<(), IoSubmissionError> {
        crate::submit_io_operation(
            IoOperation::cancel_forget(self.key),
            None
        )?;

        Ok(())
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
    ) -> notifications::Subscription {
         notifications::Subscription::new(
            self.notification_receiver.clone(),
            flags
        )
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

    pub fn new(id: usize) -> Result<Self, WorkerInitializationError> {

        let (tx, rx) = crossbeam_channel::unbounded();
        let message_channel = EventChannel::new()?;
        let sender = WorkSender::new(tx, message_channel.as_sender());
        let poller = EventPoller::new(POLLER_BUFFER_SIZE)?;
        let event_registry = poller.registry();
        let message_channel_key: Key = event_registry.register_interest(
            EventSource::new(&message_channel),
            InterestType::READ
        ).map_err(|e| {
            WorkerInitializationError::MessageChannelPollerRegistrationError(e)
        })?;
        let executor = Executor::new(sender.clone());
        
        let mut io_context = IoContext::new(256).map_err(|e| {
            WorkerInitializationError::IoContextCreationError(e)
        })?;

        let io_channel = EventChannel::new()?;

        io_context.register_event_channel(&io_channel).map_err(|e| {
            WorkerInitializationError::IoChannelContextRegistrationError(e)
        })?;

        let io_channel_key = event_registry.register_interest(
            EventSource::new(&io_channel),
            InterestType::READ
        ).map_err(|e| {
            WorkerInitializationError::IoChannelPollerRegistrationError(e)
        })?;

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

    pub fn submit_io_operation(
        &self,
        operation: IoOperation, 
        waker: Option<std::task::Waker>
    ) -> Result<WorkerIoSubmissionHandle, SendToWorkerChannelError> {
        let key = self.io_submission_queue.submit(operation, waker)?;
        Ok(WorkerIoSubmissionHandle::new(key, self.io_completion_queue.clone()))
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
    ) -> notifications::Subscription {
        notifications::Subscription::new(
            self.notification_receiver.clone(),
            flags
        )
    }

    pub fn start(&mut self) -> Result<(), WorkerStartError> {
        if self.state.compare_exchange(
            WorkerState::None as u8,
            WorkerState::Starting as u8,
            Ordering::SeqCst,
            Ordering::SeqCst
        ).is_err() { return Err(WorkerStartError::already_running(self.id)) }

        let state = Arc::clone(&self.state);
        let id = self.id;
        let queue_registry = self.queue_registry.clone();
        let temp_data = match self.temp_data.lock().take() {
            Some(temp_data) => temp_data,
            None => return Err(WorkerStartError::temp_data_unavailable(self.id))
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
            ) { error!("Worker stopped prematurely due to an error: {e}"); }
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

    pub fn kill(&self) -> Result<(), WorkerStopError> {
        if !self.is_running() { return Err(WorkerStopError::not_running(self.id)) }
        self.sender.send_message(Message::Kill).map_err(|e| {
            WorkerStopError::new(self.id, e.into())
        })?;
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), WorkerStopError> {
        if !self.is_running() { return Err(WorkerStopError::not_running(self.id)) }
        self.sender.send_message(Message::Shutdown).map_err(|e| {
            WorkerStopError::new(self.id, e.into())
        })?;
        Ok(())
    }

    pub fn spawn(&self, task: Task) -> Result<(), SendToWorkerChannelError> {
        Ok(self.sender.send_message(Message::SpawnTask(task))?)
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
    ) -> Result<(), async_broadcast::SendError<Notification>> {
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
    ) -> Result<(), WorkerRuntimeError> {

        let mut events: Vec<Event> = Vec::with_capacity(1024);
        let mut queues_to_wake = VecDeque::new();
        let mut should_recv: bool;
        let mut should_complete_io: bool;
        let mut should_submit_io: bool;

        let (mut message_channel_key, message_channel_receiver) = message_channel_key_and_receiver;
        let (io_channel_key, io_channel_receiver) = io_channel_key_and_receiver;

        let sfd = unsafe { 
            Self::block_async_signals().map_err(|e| {
                WorkerRuntimeError::failed_to_block_signals(id, e)
            })?
        };

        let sfd_key = poller.registry().register_interest(
            EventSource::new(&sfd),
            InterestType::READ
        ).map_err(|e| {
            WorkerRuntimeError::failed_to_register_signal_channel(id, e)
        })?;

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
                            error!("cl-async: Worker {}: Failed to poll events: {}", id, source);   
                        }
                    }
                }
                Err(e) => {
                    error!("cl-async: Worker {}: Failed to poll events: {}", id, e);
                }
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
                    
                    let ret = match syscall!(
                        read(sfd, 
                            &mut siginfo as *mut _ as *mut libc::c_void, 
                            std::mem::size_of::<libc::signalfd_siginfo>()
                        )
                    ) {
                        Ok(ret) => ret,
                        Err(e) => {
                            error!("cl-async: Worker {}: Failed to read signal info: {}", id, e);
                            continue;
                        }
                    };

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
                            _ => if let Err(e) = Self::broadcast_notification(
                                &notification_sender,
                                msg
                            ) {
                                error!("cl-async: Worker {}: Failed to broadcast notification: {}", id, e);
                            }
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
                        Message::Kill => { 
                            return Ok(()); 
                        },
                        Message::Shutdown => {
                            Self::set_state(&state, WorkerState::Stopping);
                        },
                        Message::Continue(waker) => {
                            waker.wake();
                        },
                        Message::RepairMessageChannel => {
                            match EventChannel::new() {
                                Ok(channel) => {
                                    match poller.registry().register_interest(
                                        EventSource::new(&channel),
                                        InterestType::READ
                                    ) {
                                        Ok(new_key) => {

                                            if let Err(e) = poller.registry().deregister_interest(
                                                message_channel_key
                                            ) {
                                                warn!("cl-async: Worker {}: Failed to deregister old message channel: {}", id, e);
                                            }

                                            message_channel_key = new_key;

                                            let new_fd = unsafe { channel.as_ref().release() };

                                            let old_fd = unsafe { 
                                                message_channel_receiver.as_ref().swap(new_fd)
                                            };

                                            if old_fd >= 0 {
                                                if let Err(e) = syscall!(close(old_fd)) {
                                                    error!("cl-async: Worker {}: Failed to close old message channel: {}", id, e);
                                                }
                                            }

                                            info!("cl-async: Worker {}: Repaired message channel", id);
                                        },

                                        Err(e) => {
                                            error!("cl-async: Worker {}: Failed to register new message channel: {}", id, e);
                                        }
                                        
                                    }
                                }
                                Err(e) => {
                                    error!("cl-async: Worker {}: Failed to create new message channel: {}", id, e);
                                }
                            };
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
                ).unwrap_or_else(|e| {
                    error!("cl-async: Worker {}: Failed to broadcast shutdown notification: {}", id, e);
                });
                break;
            }
        }

        info!("cl-async: Worker {} shutting down...", id);

        io_channel_receiver.drain().unwrap_or_else(|e| {
            error!("cl-async: Worker {}: Failed to drain IO channel: {}", id, e);
        });

        message_channel_receiver.drain().unwrap_or_else(|e| {
            error!("cl-async: Worker {}: Failed to drain message channel: {}", id, e);
        });

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

            if let Err(e) = io_context.blocking_submit() {
                error!("cl-async: Worker {}: Failed to submit IO operations: {}", id, e);
            };

            io_context.complete();

            while executor.has_ready_tasks() { executor.run_ready_tasks(); }
            if !executor.has_running_tasks() { break; }
        }

        queue_registry.clear();

        Ok(())
    }
}