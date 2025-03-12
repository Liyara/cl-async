#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkerState {
    None = 0,
    Starting = 1,
    Idle = 2,
    Busy = 3,
    Stopping = 4,
    Stopped = 5,
}

impl From<u8> for WorkerState {
    fn from(state: u8) -> Self {
        match state {
            0 => WorkerState::None,
            1 => WorkerState::Starting,
            2 => WorkerState::Idle,
            3 => WorkerState::Busy,
            4 => WorkerState::Stopping,
            5 => WorkerState::Stopped,
            _ => unreachable!(),
        }
    }
}