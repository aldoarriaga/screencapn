use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct History<T>
where
    T: Clone + PartialEq,
{
    undo_stack: VecDeque<T>,
    undo_weights: VecDeque<usize>,
    redo_stack: Vec<T>,
    redo_weights: Vec<usize>,
    limit: usize,
    weight_limit: Option<usize>,
    estimator: Option<fn(&T) -> usize>,
}

impl<T> History<T>
where
    T: Clone + PartialEq,
{
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            undo_weights: VecDeque::new(),
            redo_stack: Vec::new(),
            redo_weights: Vec::new(),
            limit,
            weight_limit: None,
            estimator: None,
        }
    }

    pub fn with_weight_limit(
        limit: usize,
        weight_limit: usize,
        estimator: fn(&T) -> usize,
    ) -> Self {
        Self {
            weight_limit: Some(weight_limit),
            estimator: Some(estimator),
            ..Self::new(limit)
        }
    }

    fn weight(&self, state: &T) -> usize {
        self.estimator.map(|estimate| estimate(state)).unwrap_or(0)
    }

    fn trim_undo(&mut self) {
        while self.undo_stack.len() > 1
            && (self.undo_stack.len() > self.limit
                || self
                    .weight_limit
                    .is_some_and(|limit| self.undo_weights.iter().copied().sum::<usize>() > limit))
        {
            self.undo_stack.pop_front();
            self.undo_weights.pop_front();
        }
    }

    fn trim_redo(&mut self) {
        while self.redo_stack.len() > 1
            && (self.redo_stack.len() > self.limit
                || self
                    .weight_limit
                    .is_some_and(|limit| self.redo_weights.iter().copied().sum::<usize>() > limit))
        {
            self.redo_stack.remove(0);
            self.redo_weights.remove(0);
        }
    }

    pub fn checkpoint(&mut self, state: &T) {
        if self.limit == 0 {
            self.redo_stack.clear();
            self.redo_weights.clear();
            return;
        }
        self.redo_stack.clear();
        self.redo_weights.clear();
        if self
            .undo_stack
            .back()
            .is_some_and(|previous| previous == state)
        {
            return;
        }
        let weight = self.weight(state);
        self.undo_stack.push_back(state.clone());
        self.undo_weights.push_back(weight);
        self.trim_undo();
    }

    pub fn undo(&mut self, current: &T) -> Option<T> {
        let previous = self.undo_stack.pop_back()?;
        self.undo_weights.pop_back();
        let weight = self.weight(current);
        self.redo_stack.push(current.clone());
        self.redo_weights.push(weight);
        self.trim_redo();
        Some(previous)
    }

    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo_stack.pop()?;
        self.redo_weights.pop();
        let weight = self.weight(current);
        self.undo_stack.push_back(current.clone());
        self.undo_weights.push_back(weight);
        self.trim_undo();
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

    #[test]
    fn weighted_history_evicts_old_snapshots() {
        let mut history = History::with_weight_limit(10, 6, |value: &String| value.len());
        history.checkpoint(&"aaa".to_string());
        history.checkpoint(&"bbbb".to_string());

        assert_eq!(history.undo_stack.len(), 1);
        assert_eq!(history.undo_stack.back().map(String::as_str), Some("bbbb"));
    }
}
