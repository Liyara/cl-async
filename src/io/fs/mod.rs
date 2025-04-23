use thiserror::Error;

mod file;
mod stats;
mod directory;

pub use file::File;
pub use file::IoFileFuture;

pub use stats::Stats;
pub use stats::IoStatsFuture;

pub use directory::Directory;
pub use directory::IoDirectoryFuture;
pub use directory::DirectoryEntry;
pub use directory::DirectoryStream;

use super::{operation_data::IoFileOpenSettings, IoError};

#[derive(Debug, Error)]
pub enum FileSystemError {
    #[error("IO Error: {0}")]
    Io(#[from] IoError),

    #[error("Invalid file open settings: {0}")]
    InvalidFileOpenSettings(IoFileOpenSettings),
}

pub type FileSystemResult<T> = std::result::Result<T, FileSystemError>;