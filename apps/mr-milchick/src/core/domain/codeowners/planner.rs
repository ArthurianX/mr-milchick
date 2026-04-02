use std::collections::{BTreeMap, BTreeSet};

use crate::core::domain::codeowners::matcher::collect_section_requirements_for_snapshot;
use crate::core::domain::codeowners::model::{
    CodeownersAssignmentPlan, CodeownersFile, CoverageGap, MatchedSectionRequirement,
};
use crate::core::model::ReviewSnapshot;

pub fn plan_codeowners_assignments(
    codeowners: &CodeownersFile,
    snapshot: &ReviewSnapshot,
) -> CodeownersAssignmentPlan {
    let author = snapshot.author.username.as_str();
    let current_reviewers = snapshot.reviewer_usernames();

    let matched_sections = normalize_requirements(
        collect_section_requirements_for_snapshot(codeowners, snapshot),
        author,
    );

    if matched_sections.is_empty() {
        return CodeownersAssignmentPlan {
            matched_sections,
            assigned_reviewers: Vec::new(),
            uncovered_sections: Vec::new(),
            reasons: vec![
                "No CODEOWNERS approval requirements matched this merge request.".to_string(),
            ],
        };
    }

    let mut reasons = Vec::new();
    let mut unmet = unmet_demand_by_section(&matched_sections, &current_reviewers);
    let mut selected = BTreeSet::new();

    for reviewer in &current_reviewers {
        selected.insert(reviewer.clone());
    }

    let mut additional_assignments = Vec::new();
    let candidate_pool = collect_candidate_pool(&matched_sections);

    while has_unmet_demand(&unmet) {
        let Some(next_reviewer) =
            select_best_candidate(&matched_sections, &unmet, &selected, &candidate_pool)
        else {
            break;
        };

        selected.insert(next_reviewer.clone());
        additional_assignments.push(next_reviewer.clone());
        let covered_sections = sections_covered_by(&matched_sections, &unmet, &next_reviewer);

        for section_id in covered_sections.keys() {
            if let Some(remaining) = unmet.get_mut(section_id) {
                *remaining = remaining.saturating_sub(1);
            }
        }

        let section_names = covered_sections
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        reasons.push(format!(
            "Selected reviewer '{}' because they reduce unmet CODEOWNERS coverage for {}.",
            next_reviewer, section_names
        ));
    }

    let uncovered_sections = matched_sections
        .iter()
        .filter_map(|section| {
            let remaining = *unmet.get(&section.section_id).unwrap_or(&0);
            if remaining == 0 {
                return None;
            }

            Some(CoverageGap {
                section_name: section.section_name.clone(),
                required_approvals: section.required_approvals,
                eligible_users: section.eligible_users.clone(),
                reachable_approvals: section.required_approvals.saturating_sub(remaining),
            })
        })
        .collect::<Vec<_>>();

    if uncovered_sections.is_empty() {
        reasons.push("CODEOWNERS coverage requirements are fully satisfied by existing and planned reviewers.".to_string());
    } else {
        for gap in &uncovered_sections {
            reasons.push(format!(
                "Section '{}' still needs {} approval slot(s) but only {} can be covered with the available eligible reviewers.",
                gap.section_name,
                gap.required_approvals.saturating_sub(gap.reachable_approvals),
                gap.reachable_approvals
            ));
        }
    }

    CodeownersAssignmentPlan {
        matched_sections,
        assigned_reviewers: additional_assignments,
        uncovered_sections,
        reasons,
    }
}

fn normalize_requirements(
    requirements: Vec<MatchedSectionRequirement>,
    author_username: &str,
) -> Vec<MatchedSectionRequirement> {
    requirements
        .into_iter()
        .filter_map(|mut requirement| {
            requirement.eligible_users = requirement
                .eligible_users
                .into_iter()
                .filter(|username| username != author_username)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            if requirement.eligible_users.is_empty() {
                return Some(requirement);
            }

            Some(requirement)
        })
        .collect()
}

fn unmet_demand_by_section(
    requirements: &[MatchedSectionRequirement],
    current_reviewers: &[String],
) -> BTreeMap<String, usize> {
    requirements
        .iter()
        .map(|section| {
            let already_covered = current_reviewers
                .iter()
                .filter(|reviewer| section.eligible_users.iter().any(|user| user == *reviewer))
                .count();

            (
                section.section_id.clone(),
                section.required_approvals.saturating_sub(already_covered),
            )
        })
        .collect()
}

fn collect_candidate_pool(requirements: &[MatchedSectionRequirement]) -> BTreeSet<String> {
    requirements
        .iter()
        .flat_map(|section| section.eligible_users.iter().cloned())
        .collect()
}

fn has_unmet_demand(unmet: &BTreeMap<String, usize>) -> bool {
    unmet.values().any(|remaining| *remaining > 0)
}

fn select_best_candidate(
    requirements: &[MatchedSectionRequirement],
    unmet: &BTreeMap<String, usize>,
    selected: &BTreeSet<String>,
    candidate_pool: &BTreeSet<String>,
) -> Option<String> {
    let mut best: Option<(String, usize, usize)> = None;

    for candidate in candidate_pool {
        if selected.contains(candidate) {
            continue;
        }

        let covered_sections = sections_covered_by(requirements, unmet, candidate);
        let uncovered_slots = covered_sections.len();
        if uncovered_slots == 0 {
            continue;
        }

        let rarity_score = covered_sections
            .keys()
            .filter_map(|section_id| {
                requirements
                    .iter()
                    .find(|section| section.section_id == *section_id)
            })
            .map(|section| section.eligible_users.len())
            .sum::<usize>();

        match &best {
            Some((best_candidate, best_slots, best_rarity))
                if *best_slots > uncovered_slots
                    || (*best_slots == uncovered_slots && *best_rarity < rarity_score)
                    || (*best_slots == uncovered_slots
                        && *best_rarity == rarity_score
                        && best_candidate <= candidate) => {}
            _ => best = Some((candidate.clone(), uncovered_slots, rarity_score)),
        }
    }

    best.map(|(candidate, _, _)| candidate)
}

fn sections_covered_by(
    requirements: &[MatchedSectionRequirement],
    unmet: &BTreeMap<String, usize>,
    reviewer: &str,
) -> BTreeMap<String, String> {
    requirements
        .iter()
        .filter(|section| {
            unmet.get(&section.section_id).copied().unwrap_or(0) > 0
                && section.eligible_users.iter().any(|user| user == reviewer)
        })
        .map(|section| (section.section_id.clone(), section.section_name.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::codeowners::model::CodeownersFile;
    use crate::core::domain::codeowners::parser::parse_codeowners_str;
    use crate::core::model::{
        Actor, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewPlatformKind,
        ReviewRef, ReviewSnapshot,
    };

    fn snapshot(paths: &[&str], author: &str, reviewers: &[&str]) -> ReviewSnapshot {
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
                username: author.to_string(),
                display_name: None,
            },
            participants: reviewers
                .iter()
                .map(|reviewer| Actor {
                    username: reviewer.to_string(),
                    display_name: None,
                })
                .collect(),
            changed_files: paths
                .iter()
                .map(|path| ChangedFile {
                    path: path.to_string(),
                    previous_path: None,
                    change_type: ChangeType::Modified,
                    additions: None,
                    deletions: None,
                    patch: None,
                })
                .collect(),
            labels: vec![],
            is_draft: false,
            default_branch: Some("develop".to_string()),
            metadata: ReviewMetadata::default(),
        }
    }

    fn parse(raw: &str) -> CodeownersFile {
        parse_codeowners_str(raw).expect("codeowners should parse")
    }

    #[test]
    fn picks_minimum_set_for_overlapping_sections() {
        let codeowners = parse(
            r#"
[Libraries][2] @anon04 @anon05 @anon06 @anon01
/packages/
[Lobby][2] @anon03 @anon04 @anon05 @anon06 @anon01
/apps/lobby/
[Matka][2] @anon02 @anon12 @anon13 @anon14 @anon01
/apps/matka/
"#,
        );

        let plan = plan_codeowners_assignments(
            &codeowners,
            &snapshot(
                &["packages/a.ts", "apps/lobby/a.ts", "apps/matka/a.ts"],
                "anon01",
                &[],
            ),
        );

        assert_eq!(
            plan.assigned_reviewers,
            vec![
                "anon04".to_string(),
                "anon05".to_string(),
                "anon02".to_string(),
                "anon12".to_string()
            ]
        );
        assert!(plan.uncovered_sections.is_empty());
    }

    #[test]
    fn existing_reviewers_reduce_additional_assignments() {
        let codeowners = parse(
            r#"
[Libraries][2] @anon04 @anon05 @anon06
/packages/
"#,
        );

        let plan = plan_codeowners_assignments(
            &codeowners,
            &snapshot(&["packages/a.ts"], "anon01", &["anon04"]),
        );

        assert_eq!(plan.assigned_reviewers, vec!["anon05".to_string()]);
        assert!(plan.uncovered_sections.is_empty());
    }

    #[test]
    fn reports_impossible_coverage() {
        let codeowners = parse(
            r#"
[Libraries][2] @anon01 @anon04
/packages/
"#,
        );

        let plan =
            plan_codeowners_assignments(&codeowners, &snapshot(&["packages/a.ts"], "anon01", &[]));

        assert_eq!(plan.assigned_reviewers, vec!["anon04".to_string()]);
        assert_eq!(plan.uncovered_sections.len(), 1);
        assert_eq!(plan.uncovered_sections[0].section_name, "Libraries");
    }
}
