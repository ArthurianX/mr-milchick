#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeArea {
    Frontend,
    Backend,
    Shared,
    DevOps,
    Documentation,
    Tests,
    Unknown,
}

impl CodeArea {
    pub fn as_str(&self) -> &'static str {
        match self {
            CodeArea::Frontend => "frontend",
            CodeArea::Backend => "backend",
            CodeArea::Shared => "packages",
            CodeArea::DevOps => "devops",
            CodeArea::Documentation => "documentation",
            CodeArea::Tests => "tests",
            CodeArea::Unknown => "unknown",
        }
    }

    pub fn from_config_key(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "frontend" | "ui" => Some(CodeArea::Frontend),
            "backend" | "api" => Some(CodeArea::Backend),
            "shared" | "packages" => Some(CodeArea::Shared),
            "devops" | "ops" | "infrastructure" => Some(CodeArea::DevOps),
            "documentation" | "docs" => Some(CodeArea::Documentation),
            "tests" | "test" | "qa" => Some(CodeArea::Tests),
            "unknown" => Some(CodeArea::Unknown),
            _ => None,
        }
    }
}
