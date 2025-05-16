use std::{
    ffi::OsStr, os::{fd::{
        AsRawFd, 
        FromRawFd, 
        RawFd
    }, unix::ffi::OsStrExt}, path::{
        Path, 
        PathBuf
    }, sync::Arc
};

use bytes::Bytes;
use thiserror::Error;

use crate::{
    io::{
        completion::TryFromCompletion, operation::future::{
            IoOperationFuture, IoReadFuture, IoVoidFuture, __async_impl_copyable__, __async_impl_readable__, __async_impl_types__, __async_impl_writable__
        }, operation_data::{
            IoFileCreateMode, 
            IoFileOpenSettings, 
            IoStatxFlags, 
            IoStatxMask
        }, 
        IoCompletion, 
        IoError, 
        IoOperation, 
        OwnedFdAsync
    }, 
    OsError
};

use super::{
    directory::MkdirError, Directory, IoStatsFuture, Stats
};

pub type IoFileFuture = IoOperationFuture<crate::io::fs::File>; 

#[derive(Clone)]
pub struct File {
    fd: Arc<OwnedFdAsync>,
    path: PathBuf,
}

impl TryFromCompletion for File {
    fn try_from_completion(completion: crate::io::IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::File(data) => {
                let fd = data.fd;
                let path = PathBuf::from(OsStr::from_bytes(&data.path));

                Some(unsafe { File::new(fd, path) })
            },
            _ => None
        }
    }
}

#[derive(Debug, Error)]
pub enum FileOpenError {
    #[error("Invalid file open settings: {0}")]
    InvalidFileOpenSettings(IoFileOpenSettings),

    #[error("Failed to create directory for file: {0}")]
    FailedToCreateDirectory(#[from] MkdirError),
    
    #[error("IO Error when attmepting to open file: {0}")]
    IoError(#[from] IoError),
}

impl File {


    unsafe fn new(fd: RawFd, path: PathBuf) -> Self {
        let fd = Arc::new(
            unsafe { OwnedFdAsync::from_raw_fd(fd) }
        );
        File { fd, path }
    }

    pub fn path(&self) -> &Path { &self.path }

    pub async fn open(
        path: &Path,
        settings: IoFileOpenSettings
    ) -> Result<Self, FileOpenError> {

        if settings.is_dir() {
            return Err(
                FileOpenError::InvalidFileOpenSettings(settings)
            );
        }

        match Self::open_unchecked(
            path, 
            settings.clone()
        ).await {
            Err(e) => {
                if let Some(OsError::NotFound) = e.as_os_error() {
                    if let IoFileCreateMode::Create(mode) = settings.mode() {
                        match path.parent() {
                            Some(parent) => {
                                Directory::mkdir_recursive(
                                    parent,
                                    *mode
                                ).await?;

                                return Ok(Self::open_unchecked(path, settings).await?)
                            },
                            None => {}
                        }
                    }
                }
                Err(FileOpenError::IoError(e))
            },
            Ok(file) => Ok(file)
        }
    }

    pub (crate) async fn open_unchecked(
        path: &Path,
        settings: IoFileOpenSettings
    ) -> Result<Self, IoError> {
        Ok(IoFileFuture::new(
            IoOperation::open(
                path, 
                settings
            )?
        ).await?)
    }

    pub async fn set_path(
        &mut self,
        new_path: &Path,
    ) -> Result<(), IoError> {
        IoVoidFuture::new(
            IoOperation::rename(
                self.path(),
                new_path
            )?
        ).await?;

        self.path = new_path.to_path_buf();

        Ok(())
    }

    pub async fn stats(
        &self,
        flags: IoStatxFlags,
        mask: IoStatxMask,
    ) -> Result<Stats, IoError> {
        Ok(IoStatsFuture::new(
            IoOperation::stats_fd(
                self,
                flags,
                mask
            )?
        ).await?.into())
    }

    pub async fn read_all(&self) -> Result<Bytes, IoError> {
        self.read_from(0).await
    }

    pub async fn read_from(
        &self,
        offset: usize
    ) -> Result<Bytes, IoError> {
        
        let size = self.stats(
            IoStatxFlags::DEFAULT,
            IoStatxMask::SIZE
        ).await?.size.unwrap_or(0) as usize;

        let len = size - offset;

        if len == 0 { return Ok(Bytes::new()); }

        Ok(IoReadFuture::new(
            IoOperation::read_at(
                self,
                offset,
                len
            )
        ).await?)
    }
    
    
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

__async_impl_types__!(File);
__async_impl_readable__!(File);
__async_impl_writable__!(File);
__async_impl_copyable__!(File);