use crate::domain::codeowners::model::{CodeownersFile, CodeownersRule};

pub fn match_owners(codeowners: &CodeownersFile, path: &str) -> Vec<String> {
    let mut matched_rule: Option<&CodeownersRule> = None;

    for rule in &codeowners.rules {
        if rule_matches(rule, path) {
            matched_rule = Some(rule);
        }
    }

    matched_rule
        .map(|rule| filter_individual_owners(&rule.owners))
        .unwrap_or_default()
}

pub fn match_usernames(codeowners: &CodeownersFile, path: &str) -> Vec<String> {
    match_owners(codeowners, path)
        .into_iter()
        .map(|owner| owner.trim_start_matches('@').to_string())
        .collect()
}

fn rule_matches(rule: &CodeownersRule, path: &str) -> bool {
    let normalized_path = normalize_path(path);
    let pattern = normalize_pattern(&rule.pattern);

    if pattern == "*" {
        return true;
    }

    if pattern.ends_with('/') {
        return normalized_path.starts_with(&pattern);
    }

    normalized_path == pattern
}

fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    }
}

fn normalize_pattern(pattern: &str) -> String {
    if pattern == "*" {
        "*".to_string()
    } else if pattern.starts_with('/') {
        pattern.to_string()
    } else {
        format!("/{}", pattern)
    }
}

fn filter_individual_owners(owners: &[String]) -> Vec<String> {
    owners
        .iter()
        .filter(|owner| is_individual_owner(owner))
        .cloned()
        .collect()
}

fn is_individual_owner(owner: &str) -> bool {
    owner.starts_with('@') && !looks_like_team_handle(owner)
}

fn looks_like_team_handle(owner: &str) -> bool {
    matches!(
        owner,
        "@frontend-maintainers" | "@frontend-approvers"
    )
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::codeowners::model::{CodeownersFile, CodeownersRule};

    fn sample_codeowners() -> CodeownersFile {
        CodeownersFile {
            rules: vec![
                CodeownersRule {
                    pattern: "/packages/".to_string(),
                    owners: vec!["@frontend-maintainers".to_string(), "@bogdan.crisu".to_string()],
                    line_number: 1,
                },
                CodeownersRule {
                    pattern: "/packages/proxy/".to_string(),
                    owners: vec!["@daniel.andrei".to_string(), "@andrei.achim".to_string()],
                    line_number: 2,
                },
                CodeownersRule {
                    pattern: "*".to_string(),
                    owners: vec!["@frontend-approvers".to_string()],
                    line_number: 3,
                },
            ],
        }
    }

    #[test]
    fn uses_last_matching_rule() {
        let codeowners = sample_codeowners();

        let owners = match_owners(&codeowners, "packages/proxy/index.ts");

        assert_eq!(
            owners,
            vec!["@daniel.andrei".to_string(), "@andrei.achim".to_string()]
        );
    }

    #[test]
    fn filters_out_team_handles() {
        let codeowners = sample_codeowners();

        let owners = match_owners(&codeowners, "packages/button.ts");

        assert_eq!(owners, vec!["@bogdan.crisu".to_string()]);
    }

    #[test]
    fn wildcard_with_only_team_handles_yields_no_individuals() {
        let codeowners = sample_codeowners();

        let owners = match_owners(&codeowners, "something/else.txt");

        assert!(owners.is_empty());
    }

    #[test]
    fn returns_usernames_without_at_prefix() {
        let codeowners = sample_codeowners();

        let owners = match_usernames(&codeowners, "packages/proxy/index.ts");

        assert_eq!(
            owners,
            vec!["daniel.andrei".to_string(), "andrei.achim".to_string()]
        );
    }
}

