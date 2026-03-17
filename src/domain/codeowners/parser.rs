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
    let mut pending_owners: Option<Vec<String>> = None;

    for (idx, line) in raw.lines().enumerate() {
        let line_number = idx + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((pattern, owners)) = parse_inline_rule(trimmed) {
            rules.push(CodeownersRule {
                pattern,
                owners,
                line_number,
            });
            pending_owners = None;
            continue;
        }

        if let Some(owners) = parse_section_owners(trimmed) {
            pending_owners = Some(owners);
            continue;
        }

        if let Some(owners) = pending_owners.take() {
            if is_pattern_only_line(trimmed) {
                rules.push(CodeownersRule {
                    pattern: trimmed.to_string(),
                    owners,
                    line_number,
                });
            }
        }
    }

    Ok(CodeownersFile { rules })
}

fn parse_inline_rule(line: &str) -> Option<(String, Vec<String>)> {
    if !is_pattern_only_line(line) {
        return None;
    }

    let mut parts = line.split_whitespace();
    let pattern = parts.next()?.to_string();
    let owners: Vec<String> = parts
        .filter(|part| part.starts_with('@'))
        .map(normalize_owner)
        .collect();

    if owners.is_empty() {
        return None;
    }

    Some((pattern, owners))
}

fn parse_section_owners(line: &str) -> Option<Vec<String>> {
    if !line.starts_with('[') {
        return None;
    }

    let owners: Vec<String> = line
        .split_whitespace()
        .filter(|part| part.starts_with('@'))
        .map(normalize_owner)
        .collect();

    if owners.is_empty() {
        return None;
    }

    Some(owners)
}

fn is_pattern_only_line(line: &str) -> bool {
    !line.is_empty() && !line.starts_with('[') && !line.starts_with('#')
}

fn normalize_owner(owner: &str) -> String {
    owner.trim().trim_start_matches('@').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_rules() {
        let raw = r#"
/packages/ @frontend-maintainers @bogdan.crisu @arthur.kovacs
/apps/lobby/ @daniel.andrei @bogdan.crisu
# comment
* @frontend-approvers
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 3);
        assert_eq!(parsed.rules[0].pattern, "/packages/");
        assert_eq!(
            parsed.rules[0].owners,
            vec!["frontend-maintainers", "bogdan.crisu", "arthur.kovacs"]
        );
        assert_eq!(parsed.rules[1].pattern, "/apps/lobby/");
        assert_eq!(
            parsed.rules[1].owners,
            vec!["daniel.andrei", "bogdan.crisu"]
        );
        assert_eq!(parsed.rules[2].pattern, "*");
        assert_eq!(parsed.rules[2].owners, vec!["frontend-approvers"]);
    }

    #[test]
    fn parses_gitlab_section_style_rules() {
        let raw = r#"
[Owner][1] @arthur.kovacs
CODEOWNERS

[Libraries][2] @bogdan.crisu @arthur.kovacs
/packages/

[Lobby__Bootstrap_Team][2] @daniel.andrei @bogdan.crisu @robert.rapiteanu @tbadescu @arthur.kovacs
/apps/lobby/
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 3);
        assert_eq!(parsed.rules[0].pattern, "CODEOWNERS");
        assert_eq!(parsed.rules[0].owners, vec!["arthur.kovacs"]);
        assert_eq!(parsed.rules[1].pattern, "/packages/");
        assert_eq!(
            parsed.rules[1].owners,
            vec!["bogdan.crisu", "arthur.kovacs"]
        );
        assert_eq!(parsed.rules[2].pattern, "/apps/lobby/");
        assert_eq!(
            parsed.rules[2].owners,
            vec![
                "daniel.andrei",
                "bogdan.crisu",
                "robert.rapiteanu",
                "tbadescu",
                "arthur.kovacs"
            ]
        );
    }

    #[test]
    fn parses_mixed_inline_and_section_rules() {
        let raw = r#"
[Libraries][2] @bogdan.crisu @arthur.kovacs
/packages/
/apps/lobby/ @daniel.andrei @bogdan.crisu
* @arthur.kovacs
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 3);
        assert_eq!(parsed.rules[0].pattern, "/packages/");
        assert_eq!(
            parsed.rules[0].owners,
            vec!["bogdan.crisu", "arthur.kovacs"]
        );
        assert_eq!(parsed.rules[1].pattern, "/apps/lobby/");
        assert_eq!(
            parsed.rules[1].owners,
            vec!["daniel.andrei", "bogdan.crisu"]
        );
        assert_eq!(parsed.rules[2].pattern, "*");
        assert_eq!(parsed.rules[2].owners, vec!["arthur.kovacs"]);
    }

    #[test]
    fn ignores_section_without_following_pattern() {
        let raw = r#"
[Libraries][2] @bogdan.crisu @arthur.kovacs
# comment

[Fallback][1] @arthur.kovacs
* @arthur.kovacs
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].pattern, "*");
        assert_eq!(parsed.rules[0].owners, vec!["arthur.kovacs"]);
    }

    #[test]
    fn resets_pending_section_when_inline_rule_appears() {
        let raw = r#"
[Libraries][2] @bogdan.crisu @arthur.kovacs
/apps/lobby/ @daniel.andrei @bogdan.crisu
/packages/
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].pattern, "/apps/lobby/");
        assert_eq!(
            parsed.rules[0].owners,
            vec!["daniel.andrei", "bogdan.crisu"]
        );
    }
}
