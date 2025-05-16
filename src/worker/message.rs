use crate::{
    io::IoSubmission, task::TaskId, Task
};


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