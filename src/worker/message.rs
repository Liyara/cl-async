use crate::{
    io::IOEntry, 
    task::TaskId, 
    Task
};


pub enum Message {
    SpawnTask(Task),
    SpawnTasks(Vec<Task>),
    WakeTask(TaskId),
    SubmitIOEntry(IOEntry),
    SubmitIOEntries(Vec<IOEntry>),
    Continue,
    Shutdown,
    Kill,
}