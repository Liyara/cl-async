mod notification;
mod signal;
mod subscription;

pub use notification::Notification;
pub use signal::Signal;
pub use subscription::Subscription;
pub use subscription::NotificationFlags;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotificationError {

    #[error("Failed to send notification")]
    SendError(#[from] async_broadcast::SendError<Notification>),

    #[error("Failed to receive notification")]
    RecvError(#[from] async_broadcast::RecvError),

    #[error("The stream has been closed")]
    StreamClosed
}

pub type NotificationResult<T> = Result<T, NotificationError>;