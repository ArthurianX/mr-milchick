use crate::domain::code_area::CodeArea;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub reviewers: ReviewerConfig,
    pub codeowners: CodeownersConfig,
    pub slack: SlackConfig,
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
    pub is_mandatory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersConfig {
    pub enabled: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackConfig {
    pub enabled: bool,
    pub webhook_url: Option<String>,
    pub channel: Option<String>,
}
