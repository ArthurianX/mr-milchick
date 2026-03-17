use crate::domain::code_area::CodeArea;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub reviewers: ReviewerConfig,
    pub codeowners: CodeownersConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerConfig {
    pub definitions: Vec<ReviewerDefinition>,
    pub max_reviewers: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerDefinition {
    pub username: String,
    pub areas: Vec<CodeArea>,
    pub is_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersConfig {
    pub enabled: bool,
    pub path: Option<String>,
}
