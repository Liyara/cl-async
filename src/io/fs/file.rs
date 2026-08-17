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
    OsError, io::{
        IoCompletion, IoError, IoOperation, OwnedFdAsync, completion::TryFromCompletion, operation::future::{
            __async_impl_copyable__, __async_impl_readable__, __async_impl_types__, __async_impl_writable__, IoOperationFuture, IoReadFuture, IoVoidFuture
        }, operation_data::{
            IoFileCreateMode, IoFileOpenSettings, IoFileSystemAccessType, IoFileSystemOpenFlags, IoStatxFlags, IoStatxMask
        }
    }
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
        settings: IoFileOpenSettings,
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

    pub fn open_sync(
        path: &Path,
        open: IoFileSystemOpenFlags,
        access_type: IoFileSystemAccessType,
        mode: IoFileCreateMode,
    ) -> Result<Self, OsError> {
        unsafe {

            let ret = match mode {
                IoFileCreateMode::DoNotCreate => libc::openat(
                    libc::AT_FDCWD,
                    path.as_os_str().as_bytes().as_ptr() as *const libc::c_char,
                    access_type as i32 | open.bits(),
                ),
                IoFileCreateMode::Create(io_file_system_mode) => libc::openat(
                    libc::AT_FDCWD,
                    path.as_os_str().as_bytes().as_ptr() as *const libc::c_char,
                    access_type as i32 | open.bits() | libc::O_CREAT,
                    libc::mode_t::from(io_file_system_mode)
                ),
            };

            if ret < 0 { return Err(OsError::last()); }

            Ok(File::new(ret, path.to_path_buf()))
        }
    }

    pub async fn open_at(
        dir: &Directory,
        path: &Path,
        settings: IoFileOpenSettings
    ) -> Result<Self, FileOpenError> {

        if settings.is_dir() {
            return Err(
                FileOpenError::InvalidFileOpenSettings(settings)
            );
        }

        match Self::open_at_unchecked(
            dir, 
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

                                return Ok(Self::open_at_unchecked(dir, path, settings).await?)
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

    pub (crate) async fn open_at_unchecked(
        dir: &Directory,
        path: &Path,
        settings: IoFileOpenSettings
    ) -> Result<Self, IoError> {
        Ok(IoFileFuture::new(
            IoOperation::open_at(
                dir,
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
        
        let size = self.stats(
            IoStatxFlags::DEFAULT,
            IoStatxMask::SIZE
        ).await?.size.unwrap_or(0) as usize;

        Ok(IoReadFuture::new(
            IoOperation::read_at(
                self,
                0,
                size
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