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
}