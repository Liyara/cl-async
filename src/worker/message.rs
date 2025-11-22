use crate::{
    io::IoSubmission, task::{task_factory::BoxTaskFactory, TaskId}, Task
};

pub enum Message {
    SpawnTaskFromFactory(BoxTaskFactory),
    SpawnTask(Task),
    SpawnTasks(Vec<Task>),
    WakeTask(TaskId),
    SubmitIO(IoSubmission),
    Continue(std::task::Waker),
    RepairMessageChannel,
    Shutdown,
    Kill,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::SpawnTaskFromFactory(_) => f.debug_tuple("SpawnTaskFromFactory").finish(),
            Message::SpawnTask(task) => f.debug_tuple("SpawnTask").field(task).finish(),
            Message::SpawnTasks(tasks) => f.debug_tuple("SpawnTasks").field(tasks).finish(),
            Message::WakeTask(task_id) => f.debug_tuple("WakeTask").field(task_id).finish(),
            Message::SubmitIO(_) => f.debug_tuple("SubmitIO").finish(),
            Message::Continue(_) => f.debug_tuple("Continue").finish(),
            Message::RepairMessageChannel => f.debug_tuple("RepairMessageChannel").finish(),
            Message::Shutdown => f.debug_tuple("Shutdown").finish(),
            Message::Kill => f.debug_tuple("Kill").finish(),
        }
    }
}