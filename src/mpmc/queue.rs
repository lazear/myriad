//! A first-in-first-out queue that supports multiple producers and multiple
//! consumers using atomics.

use super::*;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering::*};

/// Linked list node
struct Node<T> {
    data: Option<T>,
    next: *mut Node<T>,
}

impl<T> Node<T> {
    fn new(data: Option<T>) -> *mut Self {
        Box::into_raw(Box::new(Node {
            data,
            next: ptr::null_mut(),
        }))
    }
}

/// A FIFO queue
pub struct Queue<T> {
    head: AtomicPtr<Node<T>>,
    tail: AtomicPtr<Node<T>>,
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        let empty = Node::new(None);
        Queue {
            head: AtomicPtr::new(empty),
            tail: AtomicPtr::new(empty),
        }
    }
}

impl<T> LockFree<T> for Queue<T> {
    fn push(&self, data: T) {
        let new_tail = Node::new(None);
        unsafe {
            loop {
                // Tail will always point to an empty value
                let tail = self.tail.load(Acquire);

                if tail == self.tail.compare_and_swap(tail, new_tail, Release) {
                    (*tail).data = Some(data);
                    (*tail).next = new_tail;
                    break;
                }
            }
        }
    }

    fn pop(&self) -> Option<T> {
        unsafe {
            loop {
                let head = self.head.load(Acquire);
                if (*head).next.is_null() {
                    return None;
                }
                if head == self.head.compare_and_swap(head, (*head).next, Release) {
                    let mut node = Box::from_raw(head);
                    return node.data.take();
                }
            }
        }
    }

    fn len(&self) -> usize {
        let mut len = 0;
        unsafe {
            let mut head = self.head.load(Acquire);
            loop {
                if !(*head).next.is_null() {
                    head = (*head).next;
                    len += 1;
                } else {
                    return len;
                }
            }
        }
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        unsafe {
            let head = self.head.swap(ptr::null_mut(), SeqCst);
            if !head.is_null() {
                let mut node = Box::from_raw(head);
                loop {
                    if !node.next.is_null() {
                        node = Box::from_raw(node.next);
                    } else {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::{atomic::AtomicUsize, Arc};

    #[derive(Debug)]
    struct Sentinel(Arc<AtomicUsize>);
    impl Drop for Sentinel {
        fn drop(&mut self) {
            self.0.fetch_add(1, Relaxed);
        }
    }

    #[test]
    fn pop_and_drop() {
        let guard = Arc::new(AtomicUsize::new(0));
        let queue = Queue::new();

        queue.push(Sentinel(guard.clone()));
        queue.push(Sentinel(guard.clone()));
        queue.push(Sentinel(guard.clone()));
        queue.pop().unwrap();
        assert_eq!(1, guard.load(Acquire));
        queue.pop().unwrap();
        assert_eq!(2, guard.load(Acquire));
        queue.pop().unwrap();
        assert_eq!(3, guard.load(Acquire));
    }

    #[test]
    fn drop() {
        let guard = Arc::new(AtomicUsize::new(0));
        {
            let queue = Queue::new();

            queue.push(Sentinel(guard.clone()));
            queue.push(Sentinel(guard.clone()));
            queue.push(Sentinel(guard.clone()));
        }
        assert_eq!(3, guard.load(Acquire));
    }

    #[test]
    fn len() {
        let queue = Queue::new();
        for i in 0..100 {
            queue.push(i);
        }
        assert_eq!(queue.len(), 100)
    }

    #[test]
    fn sanity() {
        let queue = Queue::new();
        queue.push(10);
        queue.push(5);
        queue.push(0);
        assert_eq!(queue.pop(), Some(10));
        assert_eq!(queue.pop(), Some(5));
        assert_eq!(queue.pop(), Some(0));
        assert_eq!(queue.pop(), None);
    }
}
