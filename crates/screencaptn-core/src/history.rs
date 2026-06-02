use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct History<T>
where
    T: Clone,
{
    undo_stack: VecDeque<T>,
    redo_stack: Vec<T>,
    limit: usize,
}

impl<T> History<T>
where
    T: Clone,
{
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            limit,
        }
    }

    pub fn checkpoint(&mut self, state: &T) {
        if self.limit == 0 {
            self.redo_stack.clear();
            return;
        }
        self.undo_stack.push_back(state.clone());
        self.redo_stack.clear();
        while self.undo_stack.len() > self.limit {
            self.undo_stack.pop_front();
        }
    }

    pub fn undo(&mut self, current: &T) -> Option<T> {
        let previous = self.undo_stack.pop_back()?;
        self.redo_stack.push(current.clone());
        Some(previous)
    }

    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo_stack.pop()?;
        self.undo_stack.push_back(current.clone());
        while self.undo_stack.len() > self.limit {
            self.undo_stack.pop_front();
        }
        Some(next)
    }
}
