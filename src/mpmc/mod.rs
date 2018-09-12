use std::fmt;
use std::ops::Deref;
use std::sync::atomic::*;
use std::sync::{Arc, Condvar, Mutex};

mod queue;
mod stack;

pub fn queue<T: Send + 'static>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        data: Box::new(queue::Queue::new()),
        guard: Mutex::new(false),
        waker: Condvar::new(),
        connected: AtomicBool::new(true),
        sleepers: AtomicUsize::new(0),
    });
    (Sender::new(inner.clone()), Receiver::new(inner.clone()))
}

pub fn stack<T: Send + 'static>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        data: Box::new(stack::Stack::new()),
        guard: Mutex::new(false),
        waker: Condvar::new(),
        connected: AtomicBool::new(true),
        sleepers: AtomicUsize::new(0),
    });
    (Sender::new(inner.clone()), Receiver::new(inner.clone()))
}

pub trait LockFree<T> {
    fn push(&self, item: T);
    fn pop(&self) -> Option<T>;
    fn len(&self) -> usize;
}

struct Inner<T: Send> {
    data: Box<LockFree<T>>,
    connected: AtomicBool,
    guard: Mutex<bool>,
    waker: Condvar,
    sleepers: AtomicUsize,
}

unsafe impl<T: Send> Send for Sender<T> {}
unsafe impl<T: Send> Sync for Sender<T> {}
unsafe impl<T: Send> Send for Receiver<T> {}
unsafe impl<T: Send> Sync for Receiver<T> {}

pub struct Sender<T: Send> {
    inner: Arc<SendInner<T>>,
}

pub struct Receiver<T: Send> {
    inner: Arc<RecvInner<T>>,
}

struct SendInner<T: Send> {
    inner: Arc<Inner<T>>,
}

struct RecvInner<T: Send> {
    inner: Arc<Inner<T>>,
}

impl<T: Send> Deref for RecvInner<T> {
    type Target = Arc<Inner<T>>;
    fn deref(&self) -> &Arc<Inner<T>> {
        &self.inner
    }
}

impl<T: Send> Deref for SendInner<T> {
    type Target = Arc<Inner<T>>;
    fn deref(&self) -> &Arc<Inner<T>> {
        &self.inner
    }
}

impl<T: Send> Drop for RecvInner<T> {
    fn drop(&mut self) {
        self.inner.connected.store(false, Ordering::Release);
    }
}

impl<T: Send> Drop for SendInner<T> {
    fn drop(&mut self) {
        // Disconnect
        self.inner.connected.store(false, Ordering::Release);
        // Wake sleepers
        if self.inner.sleepers.load(Ordering::Acquire) > 0 {
            *self.inner.guard.lock().unwrap() = true;
            self.inner.waker.notify_all();
        }
    }
}

impl<T: Send> Sender<T> {
    fn new(inner: Arc<Inner<T>>) -> Sender<T> {
        Sender {
            inner: Arc::new(SendInner { inner }),
        }
    }

    pub fn send(&self, data: T) -> Result<(), T> {
        // Use stricter ordering than release, because this value
        // can be changed by dropping receivers
        if self.inner.connected.load(Ordering::Acquire) {
            self.inner.data.push(data);
            if self.inner.sleepers.load(Ordering::Acquire) > 0 {
                *self.inner.guard.lock().unwrap() = true;
                self.inner.waker.notify_one();
            }
            Ok(())
        } else {
            // Return ownership
            Err(data)
        }
    }

    pub fn size_hint(&self) -> usize {
        self.inner.data.len()
    }

    /// Close the channel
    pub fn close(self) {}
}

impl<T: Send> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        Sender {
            inner: self.inner.clone(),
        }
    }
}

#[derive(PartialEq)]
pub enum Error {
    Empty,
    Disconnected,
}

impl<T: Send> Clone for Receiver<T> {
    fn clone(&self) -> Receiver<T> {
        Receiver {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Send> Receiver<T> {
    fn new(inner: Arc<Inner<T>>) -> Receiver<T> {
        Receiver {
            inner: Arc::new(RecvInner { inner }),
        }
    }

    /// Non-blocking attempt to receive data from the channel
    pub fn try_recv(&self) -> Result<T, Error> {
        match self.inner.data.pop() {
            Some(data) => Ok(data),
            None => {
                if self.inner.connected.load(Ordering::Acquire) {
                    Err(Error::Empty)
                } else {
                    Err(Error::Disconnected)
                }
            }
        }
    }

    /// Block until data is received from the channel
    pub fn recv(&self) -> Result<T, Error> {
        match self.try_recv() {
            Ok(data) => return Ok(data),
            Err(Error::Disconnected) => return Err(Error::Disconnected),
            Err(Error::Empty) => (),
        };

        let ret;
        let mut guard = self.inner.guard.lock().unwrap();
        self.inner.sleepers.fetch_add(1, Ordering::Relaxed);
        loop {
            match self.try_recv() {
                Ok(data) => {
                    ret = Ok(data);
                    break;
                }
                Err(Error::Disconnected) => {
                    ret = Err(Error::Disconnected);
                    break;
                }
                Err(Error::Empty) => {}
            };
            guard = self.inner.waker.wait(guard).unwrap();
        }
        self.inner.sleepers.fetch_sub(1, Ordering::Relaxed);
        ret
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Disconnected => write!(f, "Receiver Error: channel is disconnected"),
            Error::Empty => write!(f, "Receiver Error: channel is empty"),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Disconnected => write!(f, "Receiver Error: channel is disconnected"),
            Error::Empty => write!(f, "Receiver Error: channel is empty"),
        }
    }
}
