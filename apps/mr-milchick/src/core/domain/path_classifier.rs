use crate::core::model::{AreaDefinition, AreasConfig};

pub fn classify_path(path: &str, areas: &AreasConfig) -> Option<String> {
    areas
        .definitions
        .iter()
        .find(|area| area_matches_path(area, path))
        .map(|area| area.key.clone())
}

pub fn area_matches_path(area: &AreaDefinition, path: &str) -> bool {
    area.paths
        .iter()
        .any(|pattern| path_matches_pattern(path, pattern))
}

fn path_matches_pattern(path: &str, pattern: &str) -> bool {
    let path = normalize_path(path);
    let pattern = normalize_path(pattern);

    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }

    path == pattern
}

fn normalize_path(value: &str) -> String {
    value.trim().trim_start_matches("./").replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::AreaRisk;

    fn area(key: &str, paths: &[&str]) -> AreaDefinition {
        AreaDefinition {
            key: key.to_string(),
            paths: paths.iter().map(|path| path.to_string()).collect(),
            risk: AreaRisk::Medium,
            critical: false,
        }
    }

    #[test]
    fn detects_prefix_globs() {
        let areas = AreasConfig {
            definitions: vec![area("roulette", &["apps/roulette/**"])],
        };

        assert_eq!(
            classify_path("apps/roulette/src/main.rs", &areas),
            Some("roulette".to_string())
        );
    }

    #[test]
    fn detects_exact_paths() {
        let areas = AreasConfig {
            definitions: vec![area("ci", &[".gitlab-ci.yml"])],
        };

        assert_eq!(
            classify_path(".gitlab-ci.yml", &areas),
            Some("ci".to_string())
        );
    }
}
