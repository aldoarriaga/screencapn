use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct History<T>
where
    T: Clone + PartialEq,
{
    undo_stack: VecDeque<T>,
    redo_stack: Vec<T>,
    limit: usize,
}

impl<T> History<T>
where
    T: Clone + PartialEq,
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
        self.redo_stack.clear();
        if self
            .undo_stack
            .back()
            .is_some_and(|previous| previous == state)
        {
            return;
        }
        self.undo_stack.push_back(state.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_skips_duplicate_top_state() {
        let mut history = History::new(10);
        history.checkpoint(&1);
        history.checkpoint(&1);

        assert_eq!(history.undo_stack.len(), 1);
    }

    #[test]
    fn duplicate_checkpoint_still_clears_redo_branch() {
        let mut history = History::new(10);
        history.checkpoint(&1);
        let current = 2;
        let previous = history.undo(&current).expect("undo state");
        assert_eq!(previous, 1);
        assert_eq!(history.redo_stack.len(), 1);

        history.checkpoint(&1);

        assert!(history.redo_stack.is_empty());
        assert_eq!(history.undo_stack.len(), 1);
    }
}
