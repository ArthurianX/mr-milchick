use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::domain::codeowners::model::{CodeownersFile, CodeownersRule};

pub fn parse_codeowners_file(path: impl AsRef<Path>) -> Result<CodeownersFile> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read CODEOWNERS file '{}'", path.display()))?;

    parse_codeowners_str(&raw)
}

pub fn parse_codeowners_str(raw: &str) -> Result<CodeownersFile> {
    let mut rules = Vec::new();

    for (idx, line) in raw.lines().enumerate() {
        let line_number = idx + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Ignore metadata-ish lines like:
        // [Owner][1] @arthur.kovacs
        // ^[Frontend_Approvers][3] @frontend-approvers
        if !looks_like_pattern_line(trimmed) {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let Some(pattern) = parts.next() else {
            continue;
        };

        let owners: Vec<String> = parts
            .filter(|part| part.starts_with('@'))
            .map(normalize_owner)
            .collect();

        if owners.is_empty() {
            continue;
        }

        rules.push(CodeownersRule {
            pattern: pattern.to_string(),
            owners,
            line_number,
        });
    }

    Ok(CodeownersFile { rules })
}

fn looks_like_pattern_line(line: &str) -> bool {
    line.starts_with('/')
        || line.starts_with('*')
        || line.starts_with("**")
        || line.starts_with("apps/")
        || line.starts_with("packages/")
}

fn normalize_owner(owner: &str) -> String {
    owner.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_realistic_codeowners_lines_and_ignores_metadata() {
        let raw = r#"
[Owner][1] @arthur.kovacs
/packages/ @frontend-maintainers @bogdan.crisu @arthur.kovacs
/apps/lobby/ @daniel.andrei @bogdan.crisu
# comment
* @frontend-approvers
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 3);
        assert_eq!(parsed.rules[0].pattern, "/packages/");
        assert_eq!(parsed.rules[1].pattern, "/apps/lobby/");
        assert_eq!(parsed.rules[2].pattern, "*");
    }
}