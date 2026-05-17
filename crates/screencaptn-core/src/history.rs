#[derive(Clone, Debug)]
pub struct History<T>
where
    T: Clone,
{
    undo_stack: Vec<T>,
    redo_stack: Vec<T>,
    limit: usize,
}

impl<T> History<T>
where
    T: Clone,
{
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit,
        }
    }

    pub fn checkpoint(&mut self, state: &T) {
        self.undo_stack.push(state.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self, current: &T) -> Option<T> {
        let previous = self.undo_stack.pop()?;
        self.redo_stack.push(current.clone());
        Some(previous)
    }

    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo_stack.pop()?;
        self.undo_stack.push(current.clone());
        Some(next)
    }
}
