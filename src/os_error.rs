use thiserror::Error;



#[derive(Debug, Error)]
pub enum OSError {

    #[error("Maximum number of file descriptors reached")]
    MaxFdReached,

    #[error("Not enough memory")]
    NotEnoughMemory,

    #[error("File could not be read")]
    FileCouldNotBeRead,

    #[error("Invalid file descriptor")]
    InvalidFd,

    #[error("File descriptor is already registered with this resource")]
    FdAlreadyRegistered,

    #[error("Invalid pointer")]
    InvalidPointer,

    #[error("Insufficient permissions")]
    PermissionDenied,

    #[error("The operartion was interrupted")]
    OperationInterrupted,

    #[error("The file descriptor has not been registered with this resource")]
    FdNotRegistered,

    #[error("Invalid operation")]
    InvalidOperation,

    #[error("Unknown OS error")]
    UnknownError,

    #[error("OS Error: {0}")]
    Generic(i32),
}

impl From<std::io::Error> for OSError {
    fn from(error: std::io::Error) -> Self {
       let os_error = match error.raw_os_error() {
           Some(code) => code,
           None => return OSError::UnknownError,
       };

       match os_error {
           libc::EINVAL => OSError::InvalidOperation,
           libc::EMFILE | libc::ENFILE => OSError::MaxFdReached,
           libc::ENOMEM => OSError::NotEnoughMemory,
           libc::EACCES => OSError::FileCouldNotBeRead,
           libc::EBADF => OSError::InvalidFd,
           libc::EEXIST => OSError::FdAlreadyRegistered,
           libc::EFAULT => OSError::InvalidPointer,
           libc::ENOENT => OSError::FdNotRegistered,
           libc::EPERM => OSError::PermissionDenied,
           libc::EINTR => OSError::OperationInterrupted,
           _ => OSError::Generic(os_error),
       }
    }
}

