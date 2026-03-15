use crate::domain::code_area::CodeArea;

pub fn classify_path(path: &str) -> CodeArea {
    let p = path.to_lowercase();

    if p.starts_with("apps/frontend")
        || p.contains("/ui/")
        || p.ends_with(".tsx")
        || p.ends_with(".jsx")
    {
        CodeArea::Frontend
    } else if p.contains("/shared/") || p.starts_with("libs/") {
        CodeArea::Shared
    } else if p.contains(".gitlab")
        || p.contains("docker")
        || p.contains("k8s")
        || p.contains("infra")
    {
        CodeArea::DevOps
    } else if p.starts_with("docs/") || p.ends_with(".md") {
        CodeArea::Documentation
    } else if p.contains("test") || p.contains("spec") {
        CodeArea::Tests
    } else if p.starts_with("services/")
        || p.contains("/backend/")
        || p.ends_with(".rs")
        || p.ends_with(".go")
    {
        CodeArea::Backend
    } else {
        CodeArea::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_frontend() {
        assert_eq!(classify_path("apps/frontend/button.tsx"), CodeArea::Frontend);
    }

    #[test]
    fn detects_backend() {
        assert_eq!(classify_path("services/api/main.rs"), CodeArea::Backend);
    }

    #[test]
    fn detects_shared() {
        assert_eq!(classify_path("libs/shared/util.rs"), CodeArea::Shared);
    }

    #[test]
    fn detects_docs() {
        assert_eq!(classify_path("docs/README.md"), CodeArea::Documentation);
    }

    #[test]
    fn detects_devops() {
        assert_eq!(classify_path(".gitlab-ci.yml"), CodeArea::DevOps);
    }
}