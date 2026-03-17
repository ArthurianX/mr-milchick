use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::domain::codeowners::model::{
    CodeownersFile, CodeownersRule, CodeownersSection, OwnerRef,
};

pub fn parse_codeowners_file(path: impl AsRef<Path>) -> Result<CodeownersFile> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read CODEOWNERS file '{}'", path.display()))?;

    parse_codeowners_str(&raw)
}

pub fn parse_codeowners_str(raw: &str) -> Result<CodeownersFile> {
    let mut sections = Vec::new();
    let mut rules = Vec::new();
    let mut current_section: Option<CodeownersSection> = None;

    for (idx, line) in raw.lines().enumerate() {
        let line_number = idx + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(section) = parse_section_header(trimmed, line_number) {
            sections.push(section.clone());
            current_section = Some(section);
            continue;
        }

        if let Some((pattern, explicit_owners)) = parse_inline_rule(trimmed) {
            let owners = if explicit_owners.is_empty() {
                current_section
                    .as_ref()
                    .map(|section| section.default_owners.clone())
                    .unwrap_or_default()
            } else {
                explicit_owners
            };

            rules.push(CodeownersRule {
                pattern,
                owners,
                line_number,
                section_id: current_section.as_ref().map(|section| section.id.clone()),
            });
            continue;
        }

        if is_pattern_only_line(trimmed) {
            let owners = current_section
                .as_ref()
                .map(|section| section.default_owners.clone())
                .unwrap_or_default();

            rules.push(CodeownersRule {
                pattern: trimmed.to_string(),
                owners,
                line_number,
                section_id: current_section.as_ref().map(|section| section.id.clone()),
            });
        }
    }

    Ok(CodeownersFile { sections, rules })
}

fn parse_inline_rule(line: &str) -> Option<(String, Vec<OwnerRef>)> {
    if !is_pattern_only_line(line) {
        return None;
    }

    let mut parts = line.split_whitespace();
    let pattern = parts.next()?.to_string();
    let owners: Vec<OwnerRef> = parts.map(parse_owner_ref).collect();

    if owners.is_empty() && line.split_whitespace().count() > 1 {
        return None;
    }

    Some((pattern, owners))
}

fn parse_section_header(line: &str, line_number: usize) -> Option<CodeownersSection> {
    if !line.starts_with('[') {
        return None;
    }

    let header_end = line.find(']')?;
    let raw_name = &line[1..header_end];
    let optional = raw_name.starts_with('^');
    let name = if optional { &raw_name[1..] } else { raw_name }.trim();
    if name.is_empty() {
        return None;
    }

    let remainder = &line[header_end + 1..];
    let remainder = remainder.trim_start();
    if !remainder.starts_with('[') {
        return None;
    }

    let approvals_end = remainder.find(']')?;
    let approvals_raw = remainder[1..approvals_end].trim();
    let required_approvals = approvals_raw.parse::<usize>().unwrap_or(1).max(1);
    let owners: Vec<OwnerRef> = remainder[approvals_end + 1..]
        .split_whitespace()
        .map(parse_owner_ref)
        .collect();

    Some(CodeownersSection {
        id: section_id(name),
        name: name.to_string(),
        required_approvals,
        optional,
        line_number,
        default_owners: owners,
    })
}

fn is_pattern_only_line(line: &str) -> bool {
    !line.is_empty() && !line.starts_with('[') && !line.starts_with('#')
}

fn parse_owner_ref(owner: &str) -> OwnerRef {
    let normalized = owner.trim();

    if normalized.starts_with('@') {
        OwnerRef::User(normalized.trim_start_matches('@').to_string())
    } else if normalized.starts_with("group:") {
        OwnerRef::Group(normalized.to_string())
    } else {
        OwnerRef::Role(normalized.to_string())
    }
}

fn section_id(name: &str) -> String {
    name.to_ascii_lowercase()
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

        assert!(parsed.sections.is_empty());
        assert_eq!(parsed.rules.len(), 3);
        assert_eq!(parsed.rules[0].pattern, "/packages/");
        assert_eq!(
            parsed.rules[0].owners,
            vec![
                OwnerRef::User("frontend-maintainers".to_string()),
                OwnerRef::User("bogdan.crisu".to_string()),
                OwnerRef::User("arthur.kovacs".to_string())
            ]
        );
        assert_eq!(parsed.rules[1].pattern, "/apps/lobby/");
        assert_eq!(
            parsed.rules[1].owners,
            vec![
                OwnerRef::User("daniel.andrei".to_string()),
                OwnerRef::User("bogdan.crisu".to_string())
            ]
        );
        assert_eq!(parsed.rules[2].pattern, "*");
        assert_eq!(
            parsed.rules[2].owners,
            vec![OwnerRef::User("frontend-approvers".to_string())]
        );
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

        assert_eq!(parsed.sections.len(), 3);
        assert_eq!(parsed.rules.len(), 3);
        assert_eq!(parsed.rules[0].pattern, "CODEOWNERS");
        assert_eq!(
            parsed.rules[0].owners,
            vec![OwnerRef::User("arthur.kovacs".to_string())]
        );
        assert_eq!(parsed.rules[0].section_id.as_deref(), Some("owner"));
        assert_eq!(parsed.rules[1].pattern, "/packages/");
        assert_eq!(
            parsed.rules[1].owners,
            vec![
                OwnerRef::User("bogdan.crisu".to_string()),
                OwnerRef::User("arthur.kovacs".to_string())
            ]
        );
        assert_eq!(parsed.rules[2].pattern, "/apps/lobby/");
        assert_eq!(
            parsed.rules[2].owners,
            vec![
                OwnerRef::User("daniel.andrei".to_string()),
                OwnerRef::User("bogdan.crisu".to_string()),
                OwnerRef::User("robert.rapiteanu".to_string()),
                OwnerRef::User("tbadescu".to_string()),
                OwnerRef::User("arthur.kovacs".to_string())
            ]
        );
        assert_eq!(parsed.sections[1].required_approvals, 2);
    }

    #[test]
    fn parses_multiple_paths_inside_one_section() {
        let raw = r#"
[Libraries][2] @bogdan.crisu @arthur.kovacs
/packages/
/libs/
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 2);
        assert_eq!(parsed.rules[0].pattern, "/packages/");
        assert_eq!(
            parsed.rules[0].owners,
            vec![
                OwnerRef::User("bogdan.crisu".to_string()),
                OwnerRef::User("arthur.kovacs".to_string())
            ]
        );
        assert_eq!(parsed.rules[1].pattern, "/libs/");
        assert_eq!(parsed.rules[1].section_id.as_deref(), Some("libraries"));
    }

    #[test]
    fn inline_rule_inside_section_keeps_section_membership() {
        let raw = r#"
[Libraries][2] @bogdan.crisu @arthur.kovacs
/apps/lobby/ @daniel.andrei @bogdan.crisu
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].pattern, "/apps/lobby/");
        assert_eq!(parsed.rules[0].section_id.as_deref(), Some("libraries"));
        assert_eq!(
            parsed.rules[0].owners,
            vec![
                OwnerRef::User("daniel.andrei".to_string()),
                OwnerRef::User("bogdan.crisu".to_string())
            ]
        );
    }

    #[test]
    fn parses_invalid_section_count_as_one() {
        let raw = r#"
[Libraries][x] @bogdan.crisu @arthur.kovacs
/packages/
"#;

        let parsed = parse_codeowners_str(raw).expect("should parse");

        assert_eq!(parsed.sections[0].required_approvals, 1);
    }
}
