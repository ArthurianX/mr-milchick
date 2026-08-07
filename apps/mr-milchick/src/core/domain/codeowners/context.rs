use crate::config::AssignmentMode;
use crate::core::domain::codeowners::model::CodeownersFile;

#[derive(Debug, Clone)]
pub struct CodeownersContext {
    pub enabled: bool,
    pub file: Option<CodeownersFile>,
    pub assignment_mode: AssignmentMode,
}

impl CodeownersContext {
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            enabled: false,
            file: None,
            assignment_mode: AssignmentMode::Minimal,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
