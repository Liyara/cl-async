use crate::task::Task;

pub enum Message {
    NewTask(Task),
    Continue,
    Shutdown,
    Kill,
}