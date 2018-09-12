//! A last-in-first-out stack that supports multiple producers and multiple
//! consumers using atomics.

use super::*;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering::*};

struct Node<T> {
    data: Option<T>,
    next: *mut Node<T>,
}

pub struct Stack<T> {
    head: AtomicPtr<Node<T>>,
}

impl<T> Stack<T> {
    pub fn new() -> Stack<T> {
        Stack {
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }
}

impl<T> LockFree<T> for Stack<T> {
    fn push(&self, item: T) {
        let new_head = Box::into_raw(Box::new(Node {
            data: Some(item),
            next: ptr::null_mut(),
        }));
        unsafe {
            loop {
                let head = self.head.load(Acquire);
                (*new_head).next = head;
                if head == self.head.compare_and_swap(head, new_head, Relaxed) {
                    break;
                }
            }
        }
    }

    fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Acquire);
            if head.is_null() {
                return None;
            } else {
                unsafe {
                    let next = (*head).next;
                    if head == self.head.compare_and_swap(head, next, Release) {
                        let mut node = Box::from_raw(head);
                        return node.data.take();
                    }
                }
            }
        }
    }

    fn len(&self) -> usize {
        let mut len = 0;
        unsafe {
            let mut head = self.head.load(Acquire);
            loop {
                if !head.is_null() {
                    head = (*head).next;
                    len += 1;
                } else {
                    return len;
                }
            }
        }
    }
}

impl<T> Drop for Stack<T> {
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
    use std::sync::atomic::{AtomicUsize, Ordering::*};
    use std::sync::Arc;
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
        let stack = Stack::new();

        stack.push(Sentinel(guard.clone()));
        stack.push(Sentinel(guard.clone()));
        stack.push(Sentinel(guard.clone()));
        stack.pop().unwrap();
        assert_eq!(1, guard.load(Acquire));
        stack.pop().unwrap();
        assert_eq!(2, guard.load(Acquire));
        stack.pop().unwrap();
        assert_eq!(3, guard.load(Acquire));
        assert!(stack.pop().is_none());
    }

    #[test]
    fn drop() {
        let guard = Arc::new(AtomicUsize::new(0));
        {
            let stack = Stack::new();

            stack.push(Sentinel(guard.clone()));
            stack.push(Sentinel(guard.clone()));
            stack.push(Sentinel(guard.clone()));
        }
        assert_eq!(3, guard.load(Acquire));
    }

    #[test]
    fn len() {
        let stack = Stack::new();
        let mut len = 0;
        for i in 0..100 {
            stack.push(i);
            len += 1;
        }
        assert_eq!(stack.len(), len)
    }

    #[test]
    fn sanity() {
        let stack = Stack::new();
        stack.push(10);
        stack.push(5);
        stack.push(0);
        assert_eq!(stack.pop(), Some(0));
        assert_eq!(stack.pop(), Some(5));
        assert_eq!(stack.pop(), Some(10));
        assert_eq!(stack.pop(), None);
    }
}
