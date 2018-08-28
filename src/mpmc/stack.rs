use std::sync::atomic::{AtomicPtr, Ordering::*};

struct Node<T> {
    data: Option<T>,
    next: *mut Node<T>,
}

pub struct Stack<T> {
    head: AtomicPtr<Node<T>>,
}