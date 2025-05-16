mod file;
mod stats;
mod directory;

pub use file::File;
pub use file::IoFileFuture;
pub use file::FileOpenError;

pub use stats::Stats;
pub use stats::IoStatsFuture;

pub use directory::Directory;
pub use directory::IoDirectoryFuture;
pub use directory::DirectoryEntry;
pub use directory::DirectoryStream;
pub use directory::MkdirError;
pub use directory::OpenDirectoryError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoFileSystemError {
    #[error("Failed to create directory: {0}")]
    MkdirError(#[from] MkdirError),

    #[error("Failed to open file: {0}")]
    OpenFileError(#[from] FileOpenError),

    #[error("Failed to open directory: {0}")]
    OpenDirectoryError(#[from] OpenDirectoryError),
}