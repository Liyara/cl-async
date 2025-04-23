use std::{ffi::{OsStr, OsString}, os::{fd::{AsRawFd, FromRawFd, RawFd}, unix::ffi::OsStrExt}, path::{Path, PathBuf}, sync::Arc};

use crate::{io::{completion::TryFromCompletion, operation::future::{IoOperationFuture, IoVoidFuture}, operation_data::{IoFileDescriptorType, IoFileOpenSettings, IoFileSystemMode, IoStatxFlags, IoStatxMask}, IoCompletion, IoError, IoOperation, IoOperationError, IoResult, OwnedFdAsync}, OsError};

use super::{IoStatsFuture, Stats};

pub type IoDirectoryFuture = IoOperationFuture<crate::io::fs::Directory>;

#[derive(Clone)]
pub struct Directory {
    fd: Arc<OwnedFdAsync>,
    path: PathBuf,
}

impl Directory {

    pub async fn check(path: &Path) -> Result<Option<IoFileSystemMode>, IoError> {
        let result = IoStatsFuture::new(
            IoOperation::stats_path(
                path,
                IoStatxFlags::DEFAULT,
                IoStatxMask::MODE
            )?
        ).await;

        let stats = match result {

            Ok(stats) => stats,

            // ENOENT
            Err(IoError::Operation(IoOperationError::Os(OsError::NotFound))) => {
                return Ok(None);
            },

            Err(e) => {
                return Err(e);
            }
        };

        match stats.descriptor_type {
            Some(IoFileDescriptorType::DIRECTORY) => {},
            Some(_) => {
                return Err(IoOperationError::Os(OsError::NotADirectory).into());
            },
            None => {
                return Err(IoOperationError::NoData.into());
            }
        }

        match stats.mode {
            Some(mode) => Ok(Some(mode)),
            None => Err(IoOperationError::NoData.into())
        }

    }

    pub async fn mkdir_recursive(
        path: &Path,
        desired_mode: IoFileSystemMode,
    ) -> Result<(), IoError>  {

        match Self::mkdir(path, desired_mode).await {

            Ok(()) => Ok(()),

            Err(e) => {
                match e {
                    IoError::Operation(
                        IoOperationError::Os(OsError::NotFound)
                    ) => {        
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
                    },
                    _ => Err(e)
                }       
            }
        }
        
    }

    pub async fn mkdir(
        path: &Path,
        desired_mode: IoFileSystemMode,
    ) -> Result<(), IoError> {
        if let Err(e) = IoVoidFuture::new(
            IoOperation::mkdir(
                path,
                desired_mode.to_directory_safe()
            )?
        ).await {
            // EEXIST
            if let IoError::Operation(
                IoOperationError::Os(OsError::AlreadyExists)
            ) = e {

                let result = IoStatsFuture::new(
                    IoOperation::stats_path(
                        path,
                        IoStatxFlags::DEFAULT,
                        IoStatxMask::TYPE
                    )?
                ).await?;

                match result.descriptor_type {
                    Some(IoFileDescriptorType::DIRECTORY) => {
                        Ok(())
                    },
                    Some(_) => {
                        Err(IoOperationError::Os(OsError::NotADirectory).into())
                    },
                    None => {
                        // Redundant check, this should never happen
                        Err(IoOperationError::NoData.into())
                    }
                }
            } 
            else { Err(e) }
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
    ) -> Result<Self, IoError> {
        Self::mkdir_recursive(path, mode).await?;
        Self::open_unchecked(path).await
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

    unsafe fn _iter(fd: RawFd) -> Result<DirectoryStream, IoError> {
        let dir_ptr = unsafe { libc::fdopendir(fd) };
        if dir_ptr.is_null() {
            return Err(IoError::Operation(
                IoOperationError::Os(OsError::last()),
            ));
        }

        Ok(DirectoryStream { dir_ptr })
    }

    pub fn iter(&self) -> Result<DirectoryStream, IoError> {

        let fd = syscall!(dup(
            self.as_raw_fd()
        )).map_err(|e| {
            IoOperationError::Os(OsError::from(e))
        })?;
        
        let result = unsafe { Self::_iter(fd) };

        if let Err(e) = result {
            syscall!(close(fd)).map_err(|e| {
                IoOperationError::Os(OsError::from(e))
            })?;
            Err(e)
        } else { result }
    }

}

impl TryFromCompletion for Directory {
    fn try_from_completion(completion: crate::io::IoCompletion) -> Result<Self, crate::io::IoError> {
        match completion {
            IoCompletion::File(data) => {
                Ok(unsafe { Directory::new(
                    &data.path,
                    data.fd
                ) })
            },
            _ => {
                Err(IoOperationError::UnexpectedPollResult(
                    String::from("Directory future got unexpected poll result")
                ).into())
            }
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
    type Item = IoResult<DirectoryEntry>;
    
    fn next(&mut self) -> Option<Self::Item> {

        unsafe { *libc::__errno_location() = 0 };

        loop {
            let entry = unsafe { libc::readdir(self.dir_ptr) };

            if entry.is_null() {
                let errno = unsafe { *libc::__errno_location() };
                if errno != 0 {
                    return Some(Err(IoError::Operation(
                        IoOperationError::Os(OsError::from(errno)),
                    )));
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