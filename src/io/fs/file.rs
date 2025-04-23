use std::{
    os::fd::{
        AsRawFd, 
        FromRawFd, 
        RawFd
    }, 
    path::{
        Path, 
        PathBuf
    }, 
    sync::Arc
};

use crate::{
    io::{
        completion::TryFromCompletion, operation::future::{
            IoOperationFuture, 
            IoReadFuture, 
            IoVoidFuture, 
            __async_impl_copyable__, 
            __async_impl_readable__, 
            __async_impl_writable__
        }, operation_data::{
            IoFileCreateMode, 
            IoFileOpenSettings, 
            IoStatxFlags, 
            IoStatxMask
        }, 
        IoCompletion, 
        IoError, 
        IoOperation, 
        IoOperationError, 
        OwnedFdAsync
    }, 
    OsError
};

use super::{
    Directory, 
    FileSystemError, 
    FileSystemResult, 
    IoStatsFuture, 
    Stats
};

pub type IoFileFuture = IoOperationFuture<crate::io::fs::File>; 

#[derive(Clone)]
pub struct File {
    fd: Arc<OwnedFdAsync>,
    path: PathBuf,
}

impl TryFromCompletion for File {
    fn try_from_completion(completion: crate::io::IoCompletion) -> Result<Self, crate::io::IoError> {
        match completion {
            IoCompletion::File(data) => {
                let fd = data.fd;
                let path = data.path;

                Ok(unsafe { File::new(fd, path) })
            },
            _ => {
                Err(IoOperationError::UnexpectedPollResult(
                    String::from("File future got unexpected poll result")
                ).into())
            }
        }
    }
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
    ) -> FileSystemResult<Self> {

        if settings.is_dir() {
            return Err(FileSystemError::InvalidFileOpenSettings(settings));
        }

        let r = Self::open_unchecked(
            path, 
            settings.clone()
        ).await;

        if let Err(FileSystemError::Io(
            IoError::Operation(IoOperationError::Os(OsError::NotFound)))
        ) = r {
            if let IoFileCreateMode::Create(mode) = settings.mode() {
                match path.parent() {
                    Some(parent) => {
                        Directory::mkdir_recursive(
                            parent,
                            *mode
                        ).await?;

                        return Self::open_unchecked(path, settings).await
                    },
                    None => {}
                }
            }

        }

        r
    }

    pub async fn open_unchecked(
        path: &Path,
        settings: IoFileOpenSettings
    ) -> FileSystemResult<Self> {
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
    ) -> FileSystemResult<()> {
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
    ) -> FileSystemResult<Stats> {
        Ok(IoStatsFuture::new(
            IoOperation::stats_fd(
                self,
                flags,
                mask
            )?
        ).await?.into())
    }

    pub async fn read_all(&self) -> FileSystemResult<Vec<u8>> {
        self.read_from(0).await
    }

    pub async fn read_from(
        &self,
        offset: usize
    ) -> FileSystemResult<Vec<u8>> {
        
        let size = match self.stats(
            IoStatxFlags::DEFAULT,
            IoStatxMask::SIZE
        ).await?.size {
            Some(size) => size,
            None => return Err(FileSystemError::Io(
                IoError::Operation(IoOperationError::NoData))
            )
        } as usize;

        let len = size - offset;

        Ok(IoReadFuture::new(
            IoOperation::read_at(
                self,
                offset,
                len
            )
        ).await?.into())
    }
    
    
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

__async_impl_readable__!(File);
__async_impl_writable__!(File);
__async_impl_copyable__!(File);