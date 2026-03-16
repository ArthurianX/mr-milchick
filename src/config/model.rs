use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MrMilchickConfig {
    pub reviewers: ReviewerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewerConfig {
    pub max_reviewers: usize,
    pub fallback_reviewers: Vec<String>,
    pub frontend: Vec<String>,
    pub backend: Vec<String>,
    pub shared: Vec<String>,
    pub devops: Vec<String>,
    pub documentation: Vec<String>,
    pub tests: Vec<String>,
}