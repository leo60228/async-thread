//! This crate provides an API identical to `std::thread`. However, `JoinHandle::join` is an `async
//! fn`.
//! ```
//! let handle = crate::spawn(|| 5usize);
//! assert_eq!(handle.join().await.map_err(drop), Ok(5));
//! ```

use futures_channel::oneshot;
use std::any::Any;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::thread as sync;

/// An owned permission to join on a thread (block on its termination).
///
/// A `JoinHandle` *detaches* the associated thread when it is dropped, which
/// means that there is no longer any handle to thread and no way to `join`
/// on it.
///
/// Due to platform restrictions, it is not possible to `Clone` this
/// handle: the ability to join a thread is a uniquely-owned permission.
///
/// This `struct` is created by the `thread::spawn` function and the
/// `thread::Builder::spawn` method.
///
/// # Examples
///
/// Creation from `thread::spawn`:
///
/// ```
/// let join_handle: async_thread::JoinHandle<_> = async_thread::spawn(|| {
///     // some work here
/// });
/// ```
///
/// Creation from `thread::Builder::spawn`:
///
/// ```
/// let builder = async_thread::Builder::new();
///
/// let join_handle: async_thread::JoinHandle<_> = builder.spawn(|| {
///     // some work here
/// }).unwrap();
/// ```
///
/// Child being detached and outliving its parent:
///
/// ```no_run
/// use std::time::Duration;
///
/// let original_thread = async_thread::spawn(|| {
///     let _detached_thread = async_thread::spawn(|| {
///         // Here we sleep to make sure that the first thread returns before.
///         thread::sleep(Duration::from_millis(10));
///         // This will be called, even though the JoinHandle is dropped.
///         println!("♫ Still alive ♫");
///     });
/// });
///
/// original_thread.join().await.expect("The thread being joined has panicked");
/// println!("Original thread is joined.");
///
/// // We make sure that the new thread has time to run, before the main
/// // thread returns.
///
/// thread::sleep(Duration::from_millis(1000));
/// ```
#[derive(Debug)]
pub struct JoinHandle<T> {
    imp: sync::JoinHandle<()>,
    chan: oneshot::Receiver<sync::Result<T>>,
}

impl<T> JoinHandle<T> {
    /// Waits for the associated thread to finish.
    ///
    /// In terms of atomic memory orderings,  the completion of the associated
    /// thread synchronizes with this function returning. In other words, all
    /// operations performed by that thread are ordered before all
    /// operations that happen after `join` returns.
    ///
    /// If the child thread panics, `Err` is returned with the parameter given
    /// to `panic`.
    ///
    /// # Panics
    ///
    /// This function may panic on some platforms if a thread attempts to join
    /// itself or otherwise may create a deadlock with joining threads.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = async_thread::Builder::new();
    ///
    /// let join_handle: async_thread::JoinHandle<_> = builder.spawn(|| {
    ///     // some work here
    /// }).unwrap();
    /// join_handle.join().await.expect("Couldn't join on the associated thread");
    /// ```
    pub async fn join(self) -> sync::Result<T> {
        let ret = self.chan
            .await
            .map_err(|x| -> Box<dyn Any + Send + 'static> { Box::new(x) })
            .and_then(|x| x);
        let _ = self.imp.join(); // synchronize threads
        ret
    }

    /// Extracts a handle to the underlying thread.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = async_thread::Builder::new();
    ///
    /// let join_handle: async_thread::JoinHandle<_> = builder.spawn(|| {
    ///     // some work here
    /// }).unwrap();
    ///
    /// let thread = join_handle.thread();
    /// println!("thread id: {:?}", thread.id());
    /// ```
    pub fn thread(&self) -> &sync::Thread {
        self.imp.thread()
    }
}

/// Thread factory, which can be used in order to configure the properties of
/// a new thread.
///
/// Methods can be chained on it in order to configure it.
///
/// The two configurations available are:
///
/// - `name`: specifies an associated name for the thread
/// - `stack_size`: specifies the desired stack size for the thread
///
/// The `spawn` method will take ownership of the builder and create an
/// `io::Result` to the thread handle with the given configuration.
///
/// The `thread::spawn` free function uses a `Builder` with default
/// configuration and `unwrap`s its return value.
///
/// You may want to use `spawn` instead of `thread::spawn`, when you want
/// to recover from a failure to launch a thread, indeed the free function will
/// panic where the `Builder` method will return a `io::Result`.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// let builder = thread::Builder::new();
///
/// let handler = builder.spawn(|| {
///     // thread code
/// }).unwrap();
///
/// handler.join().unwrap();
/// ```
#[derive(Debug)]
pub struct Builder {
    imp: sync::Builder,
}

impl Builder {
    /// Generates the base configuration for spawning a thread, from which
    /// configuration methods can be chained.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = async_thread::Builder::new()
    ///                               .name("foo".into())
    ///                               .stack_size(32 * 1024);
    ///
    /// let handler = builder.spawn(|| {
    ///     // thread code
    /// }).unwrap();
    ///
    /// handler.join().await.unwrap();
    /// ```
    pub fn new() -> Self {
        Self {
            imp: sync::Builder::new(),
        }
    }

    /// Names the thread-to-be. Currently the name is used for identification
    /// only in panic messages.
    ///
    /// The name must not contain null bytes (`\0`).
    ///
    /// For more information about named threads, see
    /// the std::thread documentation.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = async_thread::Builder::new()
    ///     .name("foo".into());
    ///
    /// let handler = builder.spawn(|| {
    ///     assert_eq!(thread::current().name(), Some("foo"))
    /// }).unwrap();
    ///
    /// handler.join().await.unwrap();
    /// ```
    pub fn name(self, name: String) -> Self {
        Self {
            imp: self.imp.name(name),
        }
    }

    /// Sets the size of the stack (in bytes) for the new thread.
    ///
    /// The actual stack size may be greater than this value if
    /// the platform specifies a minimal stack size.
    ///
    /// For more information about the stack size for threads, see
    /// the std::thread documentation.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = async_thread::Builder::new().stack_size(32 * 1024);
    /// ```
    pub fn stack_size(self, size: usize) -> Self {
        Self {
            imp: self.imp.stack_size(size),
        }
    }

    /// Spawns a new thread by taking ownership of the `Builder`, and returns an
    /// `io::Result` to its `JoinHandle`.
    ///
    /// The spawned thread may outlive the caller (unless the caller thread
    /// is the main thread; the whole process is terminated when the main
    /// thread finishes). The join handle can be used to block on
    /// termination of the child thread, including recovering its panics.
    ///
    /// For a more complete documentation see `async_thread::spawn`.
    ///
    /// # Errors
    ///
    /// Unlike the `spawn` free function, this method yields an
    /// `io::Result` to capture any failure to create the thread at
    /// the OS level.
    ///
    /// # Panics
    ///
    /// Panics if a thread name was set and it contained null bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = async_thread::Builder::new();
    ///
    /// let handler = builder.spawn(|| {
    ///     // thread code
    /// }).unwrap();
    ///
    /// handler.join().await.unwrap();
    /// ```
    pub fn spawn<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        let (send, recv) = oneshot::channel();
        let handle = self.imp.spawn(move || {
            let _ = send.send(catch_unwind(AssertUnwindSafe(f)));
        })?;

        Ok(JoinHandle {
            chan: recv,
            imp: handle,
        })
    }
}

/// Spawns a new thread, returning a `JoinHandle` for it.
///
/// The join handle will implicitly *detach* the child thread upon being
/// dropped. In this case, the child thread may outlive the parent (unless
/// the parent thread is the main thread; the whole process is terminated when
/// the main thread finishes). Additionally, the join handle provides a `join`
/// method that can be used to join the child thread. If the child thread
/// panics, `join` will return an `Err` containing the argument given to
/// `panic`.
///
/// This will create a thread using default parameters of `Builder`, if you
/// want to specify the stack size or the name of the thread, use this API
/// instead.
///
/// As you can see in the signature of `spawn` there are two constraints on
/// both the closure given to `spawn` and its return value, let's explain them:
///
/// - The `'static` constraint means that the closure and its return value
///   must have a lifetime of the whole program execution. The reason for this
///   is that threads can `detach` and outlive the lifetime they have been
///   created in.
///   Indeed if the thread, and by extension its return value, can outlive their
///   caller, we need to make sure that they will be valid afterwards, and since
///   we *can't* know when it will return we need to have them valid as long as
///   possible, that is until the end of the program, hence the `'static`
///   lifetime.
/// - The `Send` constraint is because the closure will need to be passed
///   *by value* from the thread where it is spawned to the new thread. Its
///   return value will need to be passed from the new thread to the thread
///   where it is `join`ed.
///   As a reminder, the `Send` marker trait expresses that it is safe to be
///   passed from thread to thread. `Sync` expresses that it is safe to have a
///   reference be passed from thread to thread.
///
/// # Panics
///
/// Panics if the OS fails to create a thread; use `Builder::spawn`
/// to recover from such errors.
///
/// # Examples
///
/// Creating a thread.
///
/// ```
/// let handler = async_thread::spawn(|| {
///     // thread code
/// });
///
/// handler.join().await.unwrap();
/// ```
///
/// As mentioned in the std::thread documentation, threads are usually made to
/// communicate using `channels`, here is how it usually looks.
///
/// This example also shows how to use `move`, in order to give ownership
/// of values to a thread.
///
/// ```
/// use std::sync::mpsc::channel;
///
/// let (tx, rx) = channel();
///
/// let sender = async_thread::spawn(move || {
///     tx.send("Hello, thread".to_owned())
///         .expect("Unable to send on channel");
/// });
///
/// let receiver = async_thread::spawn(move || {
///     let value = rx.recv().expect("Unable to receive from channel");
///     println!("{}", value);
/// });
///
/// sender.join().await.expect("The sender thread has panicked");
/// receiver.join().await.expect("The receiver thread has panicked");
/// ```
///
/// A thread can also return a value through its `JoinHandle`, you can use
/// this to make asynchronous computations.
///
/// ```
/// let computation = async_thread::spawn(|| {
///     // Some expensive computation.
///     42
/// });
///
/// let result = computation.join().await.unwrap();
/// println!("{}", result);
/// ```
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    Builder::new().spawn(f).expect("failed to spawn thread")
}

#[cfg(test)]
mod tests {
    #[async_std::test]
    async fn it_works() {
        let handle = crate::spawn(|| 5usize);
        assert_eq!(handle.join().await.map_err(drop), Ok(5));
    }
}
