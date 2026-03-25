use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use std::collections::HashSet;

use crate::core::domain::codeowners::model::{
    CodeownersFile, CodeownersRule, MatchedSectionRequirement, OwnerRef,
};
use crate::core::model::ReviewSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedCodeownersRule {
    pub pattern: String,
    pub section_id: Option<String>,
    pub section_name: Option<String>,
    pub required_approvals: usize,
    pub eligible_users: Vec<String>,
    pub path: String,
}

#[cfg(test)]
pub fn collect_usernames_for_snapshot(
    codeowners: &CodeownersFile,
    snapshot: &ReviewSnapshot,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut owners = Vec::new();

    for requirement in collect_section_requirements_for_snapshot(codeowners, snapshot) {
        for username in requirement.eligible_users {
            if seen.insert(username.clone()) {
                owners.push(username);
            }
        }
    }

    owners
}

pub fn collect_section_requirements_for_snapshot(
    codeowners: &CodeownersFile,
    snapshot: &ReviewSnapshot,
) -> Vec<MatchedSectionRequirement> {
    let mut grouped: BTreeMap<String, MatchedSectionRequirement> = BTreeMap::new();

    for file in &snapshot.changed_files {
        let Some(rule) = match_rule(codeowners, &file.path) else {
            continue;
        };

        let requirement = grouped
            .entry(requirement_key(codeowners, rule))
            .or_insert_with(|| build_requirement(codeowners, rule));

        if !requirement
            .matched_paths
            .iter()
            .any(|path| path == &file.path)
        {
            requirement.matched_paths.push(file.path.clone());
        }
    }

    grouped.into_values().collect()
}

pub fn collect_matched_rules_for_snapshot(
    codeowners: &CodeownersFile,
    snapshot: &ReviewSnapshot,
) -> Vec<MatchedCodeownersRule> {
    snapshot
        .changed_files
        .iter()
        .filter_map(|file| {
            let rule = match_rule(codeowners, &file.path)?;
            let section = rule
                .section_id
                .as_ref()
                .and_then(|section_id| codeowners.section_by_id(section_id));

            Some(MatchedCodeownersRule {
                pattern: rule.pattern.clone(),
                section_id: rule.section_id.clone(),
                section_name: section.map(|section| section.name.clone()),
                required_approvals: section
                    .map(|section| section.required_approvals)
                    .unwrap_or(1),
                eligible_users: filter_individual_owners(&rule.owners),
                path: file.path.clone(),
            })
        })
        .collect()
}

pub fn match_owners(codeowners: &CodeownersFile, path: &str) -> Vec<String> {
    match_rule(codeowners, path)
        .map(|rule| filter_individual_owners(&rule.owners))
        .unwrap_or_default()
}

pub fn match_usernames(codeowners: &CodeownersFile, path: &str) -> Vec<String> {
    match_owners(codeowners, path)
}

pub fn match_rule<'a>(codeowners: &'a CodeownersFile, path: &str) -> Option<&'a CodeownersRule> {
    let mut first_match = None;

    for rule in codeowners.rules.iter().rev() {
        if !rule_matches(rule, path) {
            continue;
        }

        if first_match.is_none() {
            first_match = Some(rule);
        }

        if !filter_individual_owners(&rule.owners).is_empty() {
            return Some(rule);
        }
    }

    first_match
}

fn build_requirement(
    codeowners: &CodeownersFile,
    rule: &CodeownersRule,
) -> MatchedSectionRequirement {
    let eligible_users = filter_individual_owners(&rule.owners);

    if let Some(section_id) = &rule.section_id {
        if let Some(section) = codeowners.section_by_id(section_id) {
            return MatchedSectionRequirement {
                section_id: section.id.clone(),
                section_name: section.name.clone(),
                required_approvals: section.required_approvals,
                eligible_users,
                matched_paths: Vec::new(),
            };
        }
    }

    MatchedSectionRequirement {
        section_id: format!("unsectioned:{}", rule.pattern),
        section_name: format!("Unsectioned rule '{}'", rule.pattern),
        required_approvals: 1,
        eligible_users,
        matched_paths: Vec::new(),
    }
}

fn requirement_key(codeowners: &CodeownersFile, rule: &CodeownersRule) -> String {
    if let Some(section_id) = &rule.section_id {
        if codeowners.section_by_id(section_id).is_some() {
            return section_id.clone();
        }
    }

    format!("unsectioned:{}", rule.pattern)
}

fn filter_individual_owners(owners: &[OwnerRef]) -> Vec<String> {
    let mut seen = BTreeSet::new();

    owners
        .iter()
        .filter_map(|owner| owner.as_user())
        .filter_map(|owner| {
            let normalized = normalize_owner(owner);
            if normalized.is_empty() || looks_like_team_handle(&normalized) {
                None
            } else if seen.insert(normalized.clone()) {
                Some(normalized)
            } else {
                None
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
    use crate::core::domain::codeowners::model::{CodeownersFile, CodeownersRule, CodeownersSection};
    use crate::core::model::{
        Actor, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewPlatformKind,
        ReviewRef, ReviewSnapshot,
    };

    fn snapshot(paths: &[&str]) -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".to_string(),
                review_id: "1".to_string(),
                web_url: Some("https://example.test".to_string()),
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace: "group".to_string(),
                name: "project".to_string(),
                web_url: Some("https://example.test/group/project".to_string()),
            },
            title: "Test".to_string(),
            description: None,
            author: Actor {
                username: "anon01".to_string(),
                display_name: None,
            },
            participants: vec![],
            changed_files: paths
                .iter()
                .map(|path| ChangedFile {
                    path: path.to_string(),
                    change_type: ChangeType::Modified,
                    additions: None,
                    deletions: None,
                })
                .collect(),
            labels: vec![],
            is_draft: false,
            default_branch: Some("develop".to_string()),
            metadata: ReviewMetadata::default(),
        }
    }

    fn sample_codeowners() -> CodeownersFile {
        CodeownersFile {
            sections: vec![
                CodeownersSection {
                    id: "libraries".to_string(),
                    name: "Libraries".to_string(),
                    required_approvals: 2,
                    optional: false,
                    line_number: 1,
                    default_owners: vec![],
                },
                CodeownersSection {
                    id: "proxy".to_string(),
                    name: "Proxy".to_string(),
                    required_approvals: 2,
                    optional: false,
                    line_number: 2,
                    default_owners: vec![],
                },
            ],
            rules: vec![
                CodeownersRule {
                    pattern: "/packages/".to_string(),
                    owners: vec![
                        OwnerRef::User("frontend-maintainers".to_string()),
                        OwnerRef::User("anon04".to_string()),
                    ],
                    line_number: 1,
                    section_id: Some("libraries".to_string()),
                },
                CodeownersRule {
                    pattern: "/packages/proxy/".to_string(),
                    owners: vec![
                        OwnerRef::User("anon03".to_string()),
                        OwnerRef::User("andrei.achim".to_string()),
                    ],
                    line_number: 2,
                    section_id: Some("proxy".to_string()),
                },
                CodeownersRule {
                    pattern: "*".to_string(),
                    owners: vec![OwnerRef::User("frontend-approvers".to_string())],
                    line_number: 3,
                    section_id: None,
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
            vec!["anon03".to_string(), "andrei.achim".to_string()]
        );
    }

    #[test]
    fn filters_out_team_handles() {
        let codeowners = sample_codeowners();

        let owners = match_owners(&codeowners, "packages/button.ts");

        assert_eq!(owners, vec!["anon04".to_string()]);
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
            vec!["anon03".to_string(), "andrei.achim".to_string()]
        );
    }

    #[test]
    fn collects_unique_usernames_for_snapshot() {
        let codeowners = sample_codeowners();
        let snapshot = snapshot(&["packages/proxy/a.ts", "packages/proxy/b.ts"]);

        let owners = collect_usernames_for_snapshot(&codeowners, &snapshot);

        assert_eq!(
            owners,
            vec!["anon03".to_string(), "andrei.achim".to_string()]
        );
    }

    #[test]
    fn aggregates_requirements_per_section_not_per_file() {
        let codeowners = sample_codeowners();
        let snapshot = snapshot(&["packages/proxy/a.ts", "packages/proxy/b.ts"]);

        let requirements = collect_section_requirements_for_snapshot(&codeowners, &snapshot);

        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].section_name, "Proxy");
        assert_eq!(requirements[0].required_approvals, 2);
        assert_eq!(requirements[0].matched_paths.len(), 2);
    }
}
