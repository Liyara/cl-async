use crate::{
    io::IoSubmission, task::TaskId, Task
};

#[derive(Debug)]
pub enum Message {
    SpawnTask(Task),
    SpawnTasks(Vec<Task>),
    WakeTask(TaskId),
    SubmitIO(IoSubmission),
    Continue(std::task::Waker),
    RepairMessageChannel,
    Shutdown,
    Kill,
}