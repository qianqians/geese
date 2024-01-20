use std::collections::VecDeque;

pub struct Queue<T> {
    que : VecDeque<T>
}

impl <T> Queue<T> {
    pub fn new() -> Queue<T> {
        Queue {
            que: VecDeque::new()
        }
    }

    pub fn enque(&mut self, t:T) {
        self.que.push_back(t);
    }

    pub fn deque(&mut self) -> Option<T> {
        self.que.pop_front()
    }
}