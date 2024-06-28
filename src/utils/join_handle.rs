use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Future;
use tokio::task::{AbortHandle, JoinError, JoinHandle};

/// A wrapper around [`JoinHandle`] that aborts the task when the handle is dropped.
///
/// This can be useful when using join handles in `select!` for instance.
pub struct AbortingJoinHandle<T> {
    inner: JoinHandle<T>,
}

impl<T> AbortingJoinHandle<T> {
    fn new(inner: JoinHandle<T>) -> Self {
        Self { inner }
    }

    /// Abort the task associated with the handle.
    ///
    /// Awaiting a cancelled task might complete as usual if the task was
    /// already completed at the time it was cancelled, but most likely it
    /// will fail with a cancelled [`JoinError`].
    pub fn abort(&self) {
        self.inner.abort();
    }

    /// Checks if the task associated with this [`AbortingJoinHandle`] has finished.
    ///
    /// Please note that this method can return `false` even if `abort` has been
    /// called on the task. This is because the cancellation process may take
    /// some time, and this method does not return `true` until it has
    /// completed.
    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }

    /// Returns a new `AbortHandle` that can be used to remotely abort this task.
    ///
    /// Awaiting a task cancelled by the [`AbortHandle`] might complete as usual if the task was
    /// already completed at the time it was cancelled, but most likely it
    /// will fail with a cancelled [`JoinError`].
    pub fn abort_handle(&self) -> AbortHandle {
        self.inner.abort_handle()
    }
}

impl<T> Drop for AbortingJoinHandle<T> {
    // Abort on drop
    fn drop(&mut self) {
        self.inner.abort();
    }
}

impl<T> Future for AbortingJoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        JoinHandle::poll(Pin::new(&mut self.get_mut().inner), cx)
    }
}

/// A trait that provides extension methods for [`JoinHandle`].
///
/// This trait is sealed and cannot be implemented for types outside of this module.
pub trait JoinHandleExt: seal::Sealed {
    type Output;

    /// Wraps the [`JoinHandle`] in an [`AbortingJoinHandle`]. This ensures
    /// that the task is aborted when the handle is dropped.
    fn abort_on_drop(self) -> AbortingJoinHandle<Self::Output>;
}

impl<T> JoinHandleExt for JoinHandle<T> {
    type Output = T;

    fn abort_on_drop(self) -> AbortingJoinHandle<Self::Output> {
        AbortingJoinHandle::new(self)
    }
}

mod seal {
    pub trait Sealed {}

    impl<T> Sealed for super::JoinHandle<T> {}
}
