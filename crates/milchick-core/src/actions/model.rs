use crate::model::ReviewAction;

pub type Action = ReviewAction;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionPlan {
    pub actions: Vec<Action>,
}

impl ActionPlan {
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn has_fail_pipeline(&self) -> bool {
        self.actions
            .iter()
            .any(|a| matches!(a, Action::FailPipeline { .. }))
    }
}
