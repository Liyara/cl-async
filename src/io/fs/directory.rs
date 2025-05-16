use std::{ffi::{OsStr, OsString}, os::{fd::{AsRawFd, FromRawFd, RawFd}, unix::ffi::OsStrExt}, path::{Path, PathBuf}, sync::Arc};

use thiserror::Error;

use crate::{io::{completion::TryFromCompletion, operation::future::{IoOperationFuture, IoVoidFuture}, operation_data::{IoFileDescriptorType, IoFileOpenSettings, IoFileSystemMode, IoStatxFlags, IoStatxMask}, IoCompletion, IoError, IoOperation, OwnedFdAsync}, OsError};

use super::{IoStatsFuture, Stats};

pub type IoDirectoryFuture = IoOperationFuture<crate::io::fs::Directory>;

#[derive(Debug, Error)]
pub enum MkdirError {

    #[error("IO error occured while creating directory: {0}")]
    IoError(#[from] IoError),

    #[error("Object exists, but is not a directory")]
    NotADirectory,

    #[error("Failed to retrieve descriptor type for path: {0}")]
    FailedToRetrieveDescriptorType(PathBuf),
}

impl MkdirError {
    pub fn as_os_error(&self) -> Option<&OsError> {
        match self {
            MkdirError::IoError(e) => e.as_os_error(),
            _ => None,   
        }
    }
}

#[derive(Debug, Error)]
pub enum OpenDirectoryError {

    #[error("Failed to create directory for opening: {0}")]
    MkdirError(#[from] MkdirError),

    #[error("Failed to open directory: {0}")]
    OpenError(#[from] IoError),
}


#[derive(Clone)]
pub struct Directory {
    fd: Arc<OwnedFdAsync>,
    path: PathBuf,
}

impl Directory {

    pub async fn mkdir_recursive(
        path: &Path,
        desired_mode: IoFileSystemMode,
    ) -> Result<(), MkdirError>  {

        match Self::mkdir(path, desired_mode).await {

            Ok(()) => Ok(()),

            Err(e) => {
                if let Some(OsError::NotFound) = e.as_os_error() {   
                    match path.parent() {
                        Some(parent) => {

                            if parent == path || parent.as_os_str().is_empty() {
                                Err(e)
                            } else {

                                Box::pin(
                                    Self::mkdir_recursive(
                                        parent,
                                        desired_mode
                                    )
                                ).await?;

                                Self::mkdir(path, desired_mode).await
                            }
                        },
                        None => Err(e)
                    }
                } else { Err(e) }
            }
        }
        
    }

    pub async fn mkdir(
        path: &Path,
        desired_mode: IoFileSystemMode,
    ) -> Result<(), MkdirError> {
        if let Err(e) = IoVoidFuture::new(
            IoOperation::mkdir(
                path,
                desired_mode.to_directory_safe()
            ).map_err(IoError::from)?
        ).await {
            // EEXIST
            if let Some(OsError::AlreadyExists) = e.as_os_error() {

                let result = IoStatsFuture::new(
                    IoOperation::stats_path(
                        path,
                        IoStatxFlags::DEFAULT,
                        IoStatxMask::TYPE
                    ).map_err(IoError::from)?
                ).await?;

                match result.descriptor_type {
                    Some(IoFileDescriptorType::DIRECTORY) => {
                        Ok(())
                    },
                    Some(_) => {
                        Err(MkdirError::NotADirectory)
                    },
                    None => {
                        Err(MkdirError::FailedToRetrieveDescriptorType(
                            path.to_path_buf()
                        ))
                    }
                }
            } 
            else { Err(e.into()) }
        } else { Ok(()) }
    }

    unsafe fn new(
        path: &Path,
        fd: RawFd
    ) -> Self {
        let fd = Arc::new(
            unsafe { OwnedFdAsync::from_raw_fd(fd) }
        );
        Self { fd, path: path.to_path_buf() }
    }

    pub async fn open(
        path: &Path,
        mode: IoFileSystemMode,
    ) -> Result<Self, OpenDirectoryError> {
        Self::mkdir_recursive(path, mode).await?;
        Ok(Self::open_unchecked(path).await?)
    }

    pub async fn open_unchecked(path: &Path) -> Result<Self, IoError> {
        IoDirectoryFuture::new(
            IoOperation::open(
                path,
                IoFileOpenSettings::dir()
            )?
        ).await
    }

    pub fn path(&self) -> &Path { &self.path }

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
        ).await?)
    }

    unsafe fn _iter(fd: RawFd) -> Result<DirectoryStream, OsError> {
        let dir_ptr = unsafe { libc::fdopendir(fd) };
        if dir_ptr.is_null() {
            return Err(OsError::last());
        }

        Ok(DirectoryStream { dir_ptr })
    }

    pub fn iter(&self) -> Result<DirectoryStream, OsError> {

        let fd = syscall!(dup(
            self.as_raw_fd()
        )).map_err(OsError::from)?;
        
        let result = unsafe { Self::_iter(fd) };

        if let Err(e) = result {
            syscall!(close(fd)).map_err(OsError::from)?;
            Err(e)
        } else { result }
    }

}

impl TryFromCompletion for Directory {
    fn try_from_completion(completion: crate::io::IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::File(data) => {
                Some(unsafe { Directory::new(
                    &PathBuf::from(OsStr::from_bytes(&data.path)),
                    data.fd
                ) })
            },
            _ => None
        }
    }
}

impl AsRawFd for Directory {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

pub struct DirectoryEntry {
    name: OsString,
    dtype: IoFileDescriptorType,
}

impl DirectoryEntry {
    pub fn name(&self) -> &OsStr { &self.name }
    pub fn dtype(&self) -> IoFileDescriptorType { self.dtype }

    pub fn split(self) -> (OsString, IoFileDescriptorType) {
        let name = self.name;
        let dtype = self.dtype;
        (name, dtype)
    }
}

pub struct DirectoryStream {
    dir_ptr: *mut libc::DIR,
}

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        if !self.dir_ptr.is_null() {
            let res = unsafe { libc::closedir(self.dir_ptr) };
            if res < 0 {
                error!(
                    "cl-async: Failed to close directory stream: {}",
                    OsError::last()
                );
            }
        }
    }
}

impl Iterator for DirectoryStream {
    type Item = Result<DirectoryEntry, OsError>;
    
    fn next(&mut self) -> Option<Self::Item> {

        unsafe { *libc::__errno_location() = 0 };

        loop {
            let entry = unsafe { libc::readdir(self.dir_ptr) };

            if entry.is_null() {
                let errno = unsafe { *libc::__errno_location() };
                if errno != 0 {
                    return Some(Err(OsError::from(errno)));
                }
                return None;
            }

            let d_name_ptr = unsafe { (*entry).d_name.as_ptr() };
            let d_name_c_str = unsafe { std::ffi::CStr::from_ptr(d_name_ptr) };
            let d_name_bytes = d_name_c_str.to_bytes();

            if d_name_bytes == b"." || d_name_bytes == b".." { continue; }

            let d_name = OsStr::from_bytes(d_name_bytes).to_os_string();
            let d_type = IoFileDescriptorType::from(
                unsafe { (*entry).d_type }
            );

            return Some(Ok(DirectoryEntry {
                name: d_name,
                dtype: d_type,
            }));
        }
        
    }
}