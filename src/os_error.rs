use thiserror::Error;



#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum OsError {

    #[error("Maximum number of file descriptors reached")]
    MaxFdReached,

    #[error("Not enough memory")]
    NotEnoughMemory,

    #[error("The operation was canceled")]
    OperationCanceled,

    #[error("The operation is in progress")]
    OperationInProgress,

    #[error("This operation is forbidden on this object")]
    OperationForbidden,

    #[error("The operation is not supported")]
    OperationNotSupported,

    #[error("The resource is temporarily unavailable")]
    ResourceUnavailable,

    #[error("The resource is busy")]
    ResourceBusy,

    #[error("Invalid Message")]
    InvalidMessage,

    #[error("The message was too long")]
    MessageTooLong,

    #[error("Invalid file descriptor")]
    InvalidFd,

    #[error("The resource would have deadlocked")]
    Deadlock,

    #[error("The resource is too large")]
    ResourceTooLarge,

    #[error("The resource is a directory")]
    IsADirectory,

    #[error("Attempted to create a resource which already exists")]
    AlreadyExists,

    #[error("The resource was not found")]
    NotFound,

    #[error("The resource is not a directory")]
    NotADirectory,

    #[error("Invalid pointer")]
    InvalidPointer,

    #[error("Insufficient permissions")]
    PermissionDenied,

    #[error("The operartion was interrupted")]
    OperationInterrupted,

    #[error("Invalid operation")]
    InvalidOperation,

    #[error("Unknown OS error")]
    UnknownError,

    #[error("Address already in use")]
    AddressInUse,

    #[error("Address not available")]
    AddressNotAvailable,

    #[error("Address family not supported")]
    AddressFamilyNotSupported,

    #[error("Connection refused")]
    ConnectionRefused,

    #[error("Connection reset")]
    ConnectionReset,

    #[error("Connection aborted")]
    ConnectionAborted,

    #[error("Connection timed out")]
    ConnectionTimedOut,

    #[error("Destination address required")]
    DestinationAddressRequired,

    #[error("OS Error: {0}")]
    Generic(i32),
}

impl OsError {

    pub fn last() -> Self {
        let os_error = std::io::Error::last_os_error();
        match os_error.raw_os_error() {
            Some(code) => OsError::from(code),
            None => OsError::UnknownError,
        }
    }
}

impl From<std::io::Error> for OsError {
    fn from(error: std::io::Error) -> Self {
       let os_error = match error.raw_os_error() {
           Some(code) => code,
           None => return OsError::UnknownError,
       };

       Self::from(os_error)
    }
}

impl From<i32> for OsError {
    fn from(os_error: i32) -> Self {
        match os_error {
            libc::EINVAL => OsError::InvalidOperation,
            libc::EMFILE | libc::ENFILE => OsError::MaxFdReached,
            libc::ENOMEM => OsError::NotEnoughMemory,
            libc::EACCES => OsError::OperationForbidden,
            libc::EBADF => OsError::InvalidFd,
            libc::EEXIST => OsError::AlreadyExists,
            libc::EFAULT => OsError::InvalidPointer,
            libc::ENOENT => OsError::NotFound,
            libc::EPERM => OsError::PermissionDenied,
            libc::EINTR => OsError::OperationInterrupted,
            libc::ECANCELED => OsError::OperationCanceled,
            libc::ENOTDIR => OsError::NotADirectory,
            libc::EISDIR => OsError::IsADirectory,
            libc::EAGAIN => OsError::ResourceUnavailable,
            libc::EINPROGRESS => OsError::OperationInProgress,
            libc::EDEADLK => OsError::Deadlock,
            libc::EFBIG => OsError::ResourceTooLarge,
            libc::EADDRINUSE => OsError::AddressInUse,
            libc::EADDRNOTAVAIL => OsError::AddressNotAvailable,
            libc::EAFNOSUPPORT => OsError::AddressFamilyNotSupported,
            libc::ECONNREFUSED => OsError::ConnectionRefused,
            libc::ECONNRESET => OsError::ConnectionReset,
            libc::ECONNABORTED => OsError::ConnectionAborted,
            libc::ETIMEDOUT => OsError::ConnectionTimedOut,
            libc::EDESTADDRREQ => OsError::DestinationAddressRequired,
            libc::EMSGSIZE => OsError::MessageTooLong,
            libc::ENOMSG => OsError::InvalidMessage,
            _ => OsError::Generic(os_error),
       } 
    }
}

