use crate::domain::codeowners::model::CodeownersFile;

#[derive(Debug, Clone)]
pub struct CodeownersContext {
    pub enabled: bool,
    pub file: Option<CodeownersFile>,
}

impl CodeownersContext {
    pub fn empty() -> Self {
        Self {
            enabled: false,
            file: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
