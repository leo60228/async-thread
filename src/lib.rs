use futures_channel::oneshot;
use std::any::Any;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::thread as sync;

#[derive(Debug)]
pub struct JoinHandle<T> {
    imp: sync::JoinHandle<()>,
    chan: oneshot::Receiver<sync::Result<T>>,
}

impl<T> JoinHandle<T> {
    pub async fn join(self) -> sync::Result<T> {
        self.chan
            .await
            .map_err(|x| -> Box<dyn Any + Send + 'static> { Box::new(x) })
            .and_then(|x| x)
    }

    pub fn thread(&self) -> &sync::Thread {
        self.imp.thread()
    }
}

#[derive(Debug)]
pub struct Builder {
    imp: sync::Builder,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            imp: sync::Builder::new(),
        }
    }

    pub fn name(self, name: String) -> Self {
        Self {
            imp: self.imp.name(name),
        }
    }

    pub fn stack_size(self, size: usize) -> Self {
        Self {
            imp: self.imp.stack_size(size),
        }
    }

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
