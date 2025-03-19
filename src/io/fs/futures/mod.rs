mod read;
mod write;
mod multi;

pub use read::FileReadFuture;
pub use write::FileWriteFuture;
pub use multi::FileMultiFuture;
pub use multi::FileOperationResult;

pub trait ToFileFuture {
    fn to_file_future(self) -> FileFuture;
}

pub trait JoinableFileFuture {
    fn join<F>(self, other: F) -> FileMultiFuture
    where
        Self: ToFileFuture + Sized,
        F: ToFileFuture + JoinableFileFuture;
}

pub enum FileFuture {
    Read(FileReadFuture),
    Write(FileWriteFuture),
    Multi(FileMultiFuture),
}