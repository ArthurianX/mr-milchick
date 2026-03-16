use crate::domain::codeowners::model::{CodeownersFile, CodeownersRule};
use crate::gitlab::api::MergeRequestSnapshot;
use std::collections::HashSet;


pub fn collect_usernames_for_snapshot(
    codeowners: &CodeownersFile,
    snapshot: &MergeRequestSnapshot,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut owners = Vec::new();

    for file in &snapshot.changed_files {
        for username in match_usernames(codeowners, &file.new_path) {
            if seen.insert(username.clone()) {
                owners.push(username);
            }
        }
    }

    owners
}

pub fn match_owners(codeowners: &CodeownersFile, path: &str) -> Vec<String> {
    for rule in codeowners.rules.iter().rev() {
        if !rule_matches(rule, path) {
            continue;
        }

        let individual_owners = filter_individual_owners(&rule.owners);
        if !individual_owners.is_empty() {
            return individual_owners;
        }
    }

    Vec::new()
}

pub fn match_usernames(codeowners: &CodeownersFile, path: &str) -> Vec<String> {
    match_owners(codeowners, path)
}

fn filter_individual_owners(owners: &[String]) -> Vec<String> {
    owners
        .iter()
        .filter_map(|owner| {
            let normalized = normalize_owner(owner);
            if normalized.is_empty() || looks_like_team_handle(&normalized) {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn looks_like_team_handle(owner: &str) -> bool {
    matches!(owner, "frontend-maintainers" | "frontend-approvers")
}

fn normalize_owner(owner: &str) -> String {
    owner.trim().trim_start_matches('@').to_string()
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



#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::codeowners::model::{CodeownersFile, CodeownersRule};

    fn sample_codeowners() -> CodeownersFile {
        CodeownersFile {
            rules: vec![
                CodeownersRule {
                    pattern: "/packages/".to_string(),
                    owners: vec!["frontend-maintainers".to_string(), "bogdan.crisu".to_string()],
                    line_number: 1,
                },
                CodeownersRule {
                    pattern: "/packages/proxy/".to_string(),
                    owners: vec!["daniel.andrei".to_string(), "andrei.achim".to_string()],
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
            vec!["daniel.andrei".to_string(), "andrei.achim".to_string()]
        );
    }

    #[test]
    fn filters_out_team_handles() {
        let codeowners = sample_codeowners();

        let owners = match_owners(&codeowners, "packages/button.ts");

        assert_eq!(owners, vec!["bogdan.crisu".to_string()]);
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

    #[test]
    fn collects_unique_usernames_for_snapshot() {
        use crate::gitlab::api::{ChangedFile, MergeRequestDetails, MergeRequestSnapshot, MergeRequestState};

        let codeowners = sample_codeowners();

        let snapshot = MergeRequestSnapshot {
            details: MergeRequestDetails {
                iid: 1,
                title: "Test".to_string(),
                description: None,
                state: MergeRequestState::Opened,
                is_draft: false,
                web_url: "https://example.test".to_string(),
                author_username: "arthur.kovacs".to_string(),
                reviewer_usernames: vec![],
            },
            changed_files: vec![
                ChangedFile {
                    old_path: "".to_string(),
                    new_path: "packages/proxy/a.ts".to_string(),
                    is_new: false,
                    is_renamed: false,
                    is_deleted: false,
                },
                ChangedFile {
                    old_path: "".to_string(),
                    new_path: "packages/proxy/b.ts".to_string(),
                    is_new: false,
                    is_renamed: false,
                    is_deleted: false,
                },
            ],
        };

        let owners = collect_usernames_for_snapshot(&codeowners, &snapshot);

        assert_eq!(
            owners,
            vec!["daniel.andrei".to_string(), "andrei.achim".to_string()]
        );
    }

    #[test]
    fn accepts_owners_without_at_prefix() {
        let codeowners = CodeownersFile {
            rules: vec![CodeownersRule {
                pattern: "/packages/".to_string(),
                owners: vec!["bogdan.crisu".to_string(), "frontend-maintainers".to_string()],
                line_number: 1,
            }],
        };

        let owners = match_owners(&codeowners, "packages/button.ts");

        assert_eq!(owners, vec!["bogdan.crisu".to_string()]);
    }
}

