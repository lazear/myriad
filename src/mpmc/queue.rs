use std::sync::atomic::{AtomicPtr, Ordering::*};

struct Node<T> {
    data: Option<T>,
    next: *mut Node<T>,
}

pub struct Queue<T> {
    head: AtomicPtr<Node<T>>,
    tail: AtomicPtr<Node<T>>, 
}
