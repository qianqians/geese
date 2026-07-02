use std::collections::VecDeque;

#[derive(Debug, PartialEq, Eq)]
pub enum QueueError {
    Full,
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueError::Full => write!(f, "queue is full"),
        }
    }
}

impl std::error::Error for QueueError {}

pub struct Queue<T> {
    que: VecDeque<T>,
    capacity: Option<usize>,
}

impl<T> Queue<T> {
    pub fn new() -> Queue<T> {
        Queue {
            que: VecDeque::new(),
            capacity: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Queue<T> {
        Queue {
            que: VecDeque::with_capacity(capacity),
            capacity: Some(capacity),
        }
    }

    /// Returns the number of elements in the queue.
    pub fn len(&self) -> usize {
        self.que.len()
    }

    /// Returns `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.que.is_empty()
    }

    /// Enqueue an item. Returns `Err(QueueError::Full)` if the queue has a
    /// capacity limit and is currently full.
    pub fn enque(&mut self, t: T) -> Result<(), QueueError> {
        if let Some(cap) = self.capacity {
            if self.que.len() >= cap {
                return Err(QueueError::Full);
            }
        }
        self.que.push_back(t);
        Ok(())
    }

    pub fn deque(&mut self) -> Option<T> {
        self.que.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_basic() {
        let mut q = Queue::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);

        q.enque(1).unwrap();
        q.enque(2).unwrap();
        q.enque(3).unwrap();
        assert_eq!(q.len(), 3);
        assert!(!q.is_empty());
    }

    #[test]
    fn test_queue_fifo_order() {
        let mut q = Queue::new();
        q.enque("first").unwrap();
        q.enque("second").unwrap();
        q.enque("third").unwrap();

        assert_eq!(q.deque(), Some("first"));
        assert_eq!(q.deque(), Some("second"));
        assert_eq!(q.deque(), Some("third"));
        assert_eq!(q.deque(), None);
    }

    #[test]
    fn test_queue_deque_empty() {
        let mut q: Queue<i32> = Queue::new();
        assert_eq!(q.deque(), None);
    }

    #[test]
    fn test_queue_interleaved() {
        let mut q = Queue::new();
        q.enque(1).unwrap();
        q.enque(2).unwrap();
        assert_eq!(q.deque(), Some(1));
        q.enque(3).unwrap();
        assert_eq!(q.deque(), Some(2));
        assert_eq!(q.deque(), Some(3));
        assert_eq!(q.deque(), None);
        assert!(q.is_empty());
    }

    #[test]
    fn test_queue_capacity_limit() {
        let mut q = Queue::with_capacity(2);
        assert!(q.enque(1).is_ok());
        assert!(q.enque(2).is_ok());
        assert_eq!(q.enque(3), Err(QueueError::Full));
        assert_eq!(q.len(), 2);

        // deque 后可以再插入
        assert_eq!(q.deque(), Some(1));
        assert!(q.enque(4).is_ok());
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn test_queue_unlimited_capacity() {
        let mut q = Queue::new();
        for i in 0..1000 {
            assert!(q.enque(i).is_ok());
        }
        assert_eq!(q.len(), 1000);
    }
}
