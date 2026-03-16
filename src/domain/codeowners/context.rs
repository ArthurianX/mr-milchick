use crate::domain::codeowners::model::CodeownersFile;

#[derive(Debug, Clone)]
pub struct CodeownersContext {
    pub file: Option<CodeownersFile>,
}

impl CodeownersContext {
    pub fn empty() -> Self {
        Self { file: None }
    }

    pub fn is_enabled(&self) -> bool {
        self.file.is_some()
    }
}