use crate::task::{Task, TaskId};

pub enum Message {
    NewTask(Task),
    WakeTask(TaskId),
    Stolen(Vec<TaskId>),
    Continue,
    Shutdown,
    Kill,
}