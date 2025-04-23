

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Notification {
    Shutdown,
    Kill,
    Continue,
    ChildStopped,
    Custom(u64)
}