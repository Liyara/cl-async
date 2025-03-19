use std::{ffi::CString, os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd}, sync::Arc};

use thiserror::Error;
use bitflags::bitflags;
use crate::{syscall, OSError};

use super::futures::{FileReadFuture, FileWriteFuture};


#[derive(Debug, Error)]
pub enum FileError {
    
    #[error("Failed to open file: {path}: {source}")]
    FailedToOpenFile {
        source: OSError,
        path: String,
    },

    #[error("Failed to create CString: {source}")]
    FailedToCreateCString {
        source: std::ffi::NulError,
    },

    #[error("Failed to seek to offset: {offset}: {path}: ({source})")]
    FailedToSeekToPositionInFile {
        offset: usize,
        path: String,
        source: OSError,
    },

    #[error("Could not read file cursor position for file: {path}: {source}")]
    FailedToReadFileCursorPosition {
        path: String,
        source: OSError,
    },

    #[error("Failed to reead file: {path}: {source}")]
    FailedToReadFile {
        path: String,
        source: OSError,
    },

    #[error("Failed to write file: {path}: {source}")]
    FailedToWriteFile {
        path: String,
        source: OSError,
    },

    #[error("Failed to submit read operation request: {source}")]
    FailedToSubmitReadOperationRequest {
        source: crate::Error,
    },

    #[error("Failed to submit write operation request: {source}")]
    FailedToSubmitWriteOperationRequest {
        source: crate::Error,
    },

    #[error("Failed to submit io operation request: {source}")]
    FailedToSubmitIOOperationRequest {
        source: crate::Error,
    },

    #[error("Invalid data for file write")]
    InvalidWriteData,

    #[error("IO Uring failure result: {0}")]
    IOUringFailure(i32),

    #[error("Unknown IO error occurred")]
    UnknownIOError,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct AccessFlags: i32 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
    }
}

impl AccessFlags {

    pub fn is_readable(&self) -> bool {
        self.contains(AccessFlags::READ)
    }

    pub fn is_writable(&self) -> bool {
        self.contains(AccessFlags::WRITE)
    }

    fn as_libc_flags(&self) -> i32 {
          let r = self.is_readable();
          let w = self.is_writable();

          if r && w { libc::O_RDWR } 
          else if r { libc::O_RDONLY } 
          else { libc::O_WRONLY }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct CreationFlags: i32 {
        const NONE = 0;
        const CREATE = libc::O_CREAT;
        const OVERWRITE = libc::O_TRUNC;
        const APPEND = libc::O_APPEND;
    }
}

#[derive(Clone)]
pub struct File {
    fd: Arc<OwnedFd>,
    access: AccessFlags,
    path: String,
    len: usize,
    block_size: usize,
    last_access_time: libc::time_t,
    last_modification_time: libc::time_t,
}

impl File {
    pub fn open(
        path: &str, 
        access: AccessFlags, 
        create: CreationFlags
    ) -> Result<Self, FileError> {
        
        let path_cstr = CString::new(path).map_err(
            |e| FileError::FailedToCreateCString {
                source: e.into(),
            }
        )?;

        let fd = syscall!(
            open(path_cstr.as_ptr(), access.as_libc_flags() | create.bits())
        ).map_err(|e| FileError::FailedToOpenFile {
            source: e.into(),
            path: path.to_string(),
        })?;

        let mut stat: libc::stat = unsafe { std::mem::zeroed() };

        syscall!(
            fstat(fd, &mut stat)
        ).map_err(|e| FileError::FailedToOpenFile {
            source: e.into(),
            path: path.to_string(),
        })?;

        Ok(Self {
            fd: Arc::new(unsafe { OwnedFd::from_raw_fd(fd) }),
            access,
            path: path.to_string(),
            len: stat.st_size as usize,
            block_size: stat.st_blksize as usize,
            last_access_time: stat.st_atime,
            last_modification_time: stat.st_mtime,
        })
    }

    pub fn set_position(&mut self, offset: usize) -> Result<(), FileError> {
        syscall!(
            lseek(self.fd.as_raw_fd(), offset as libc::off_t, libc::SEEK_SET)
        ).map_err(|e| FileError::FailedToSeekToPositionInFile {
            source: e.into(),
            offset,
            path: self.path.clone(),
        })?;
        Ok(())
    }

    pub fn get_position(&self) -> Result<usize, FileError> {
        let offset = syscall!(
            lseek(self.fd.as_raw_fd(), 0, libc::SEEK_CUR)
        ).map_err(|e| FileError::FailedToReadFileCursorPosition {
            source: e.into(),
            path: self.path.clone(),
        })?;
        Ok(offset as usize)
    }

    pub fn move_position(&mut self, offset: isize) -> Result<(), FileError> {
        syscall!(
            lseek(self.fd.as_raw_fd(), offset as libc::off_t, libc::SEEK_CUR)
        ).map_err(|e| FileError::FailedToSeekToPositionInFile {
            source: e.into(),
            offset: offset as usize,
            path: self.path.clone(),
        })?;
        Ok(())
    }

    pub fn last_access_time(&self) -> libc::time_t { self.last_access_time }
    pub fn last_modification_time(&self) -> libc::time_t { self.last_modification_time }
    pub fn len(&self) -> usize { self.len }
    pub fn block_size(&self) -> usize { self.block_size }

    fn _read(&self, offset: Option<usize>, len: usize) -> Result<FileReadFuture, FileError> {
        if !self.access.is_readable() {
            return Err(FileError::FailedToReadFile {
                path: self.path.clone(),
                source: OSError::PermissionDenied,
            });
        }
        Ok(FileReadFuture::new(
            Arc::clone(&self.fd),
            offset,
            len,
        ))
    }

    fn _write(&self, offset: Option<usize>, data: Vec<u8>) -> Result<FileWriteFuture, FileError> {
        if !self.access.is_writable() {
            return Err(FileError::FailedToWriteFile {
                path: self.path.clone(),
                source: OSError::PermissionDenied,
            });
        }
        Ok(FileWriteFuture::new(
            self.fd.clone(),
            offset,
            data
        ))
    }

    pub fn write(&self, data: Vec<u8>) -> Result<FileWriteFuture, FileError> {
        self._write(None, data)
    }

    pub fn write_at(&self, offset: usize, data: Vec<u8>) -> Result<FileWriteFuture, FileError> {
        self._write(Some(offset), data)
    }

    pub fn read_next(&self, len: usize) -> Result<FileReadFuture, FileError> {
        self._read(None, len)
    }

    pub fn read_at(&self, offset: usize, len: usize) -> Result<FileReadFuture, FileError> {
        self._read(Some(offset), len)
    }

    pub fn read_all(&self) -> Result<FileReadFuture, FileError> {
        self.read_at(0, self.len)
    }

    pub fn read_remaining(&self) -> Result<FileReadFuture, FileError> {
        self.read_next(self.len - self.get_position()?)
    }

    
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}