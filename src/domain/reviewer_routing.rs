use std::collections::{HashMap, HashSet};

use crate::config::model::ReviewerConfig;
use crate::domain::area_summary::MergeRequestAreaSummary;
use crate::domain::code_area::CodeArea;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerRoutingConfig {
    pub reviewers_by_area: HashMap<CodeArea, Vec<String>>,
    pub fallback_reviewers: Vec<String>,
    pub max_reviewers: usize,
}

impl ReviewerRoutingConfig {
    pub fn from_config(config: &ReviewerConfig) -> Self {
        let mut reviewers_by_area = HashMap::new();
        let mut fallback_reviewers = Vec::new();

        for definition in &config.definitions {
            if definition.is_fallback {
                fallback_reviewers.push(definition.username.clone());
            }

            for area in &definition.areas {
                reviewers_by_area
                    .entry(*area)
                    .or_insert_with(Vec::new)
                    .push(definition.username.clone());
            }
        }

        Self {
            reviewers_by_area,
            fallback_reviewers,
            max_reviewers: config.max_reviewers,
        }
    }

    pub fn example() -> Self {
        let raw = crate::config::model::ReviewerConfig {
            definitions: vec![
                crate::config::model::ReviewerDefinition {
                    username: "milchick-duty".to_string(),
                    areas: vec![],
                    is_fallback: true,
                },
                crate::config::model::ReviewerDefinition {
                    username: "alice".to_string(),
                    areas: vec![CodeArea::Frontend],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "bob".to_string(),
                    areas: vec![CodeArea::Frontend],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "carol".to_string(),
                    areas: vec![CodeArea::Backend],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "dave".to_string(),
                    areas: vec![CodeArea::Backend],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "erin".to_string(),
                    areas: vec![CodeArea::Shared],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "frank".to_string(),
                    areas: vec![CodeArea::Shared],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "grace".to_string(),
                    areas: vec![CodeArea::DevOps],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "heidi".to_string(),
                    areas: vec![CodeArea::Documentation],
                    is_fallback: false,
                },
                crate::config::model::ReviewerDefinition {
                    username: "ivan".to_string(),
                    areas: vec![CodeArea::Tests],
                    is_fallback: false,
                },
            ],
            max_reviewers: 2,
        };

        Self::from_config(&raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerRecommendation {
    pub reviewers: Vec<String>,
    pub reasons: Vec<String>,
}

impl ReviewerRecommendation {
    pub fn is_empty(&self) -> bool {
        self.reviewers.is_empty()
    }
}

pub fn recommend_reviewers(
    summary: &MergeRequestAreaSummary,
    config: &ReviewerRoutingConfig,
    excluded_reviewers: &[String],
) -> ReviewerRecommendation {
    let mut reviewers = Vec::new();
    let mut reasons = Vec::new();
    let mut selected = HashSet::new();

    let significant_areas = summary.significant_areas();

    if significant_areas.is_empty() {
        if let Some(fallback) = first_non_excluded(&config.fallback_reviewers, excluded_reviewers) {
            reviewers.push(fallback.clone());
            reasons.push("No dominant area detected; fallback reviewer selected.".to_string());
        } else {
            reasons.push(
                "No dominant area detected and no eligible fallback reviewer exists.".to_string(),
            );
        }

        return ReviewerRecommendation { reviewers, reasons };
    }

    for area in significant_areas {
        if reviewers.len() >= config.max_reviewers {
            reasons.push(format!(
                "Reviewer selection reached configured limit of {}.",
                config.max_reviewers
            ));
            break;
        }

        if let Some(pool) = config.reviewers_by_area.get(&area) {
            if let Some(candidate) =
                first_non_excluded_and_unselected(pool, excluded_reviewers, &selected)
            {
                reviewers.push(candidate.clone());
                selected.insert(candidate.clone());
                reasons.push(format!(
                    "Selected reviewer '{}' for area '{}'.",
                    candidate,
                    area.as_str()
                ));
            } else {
                reasons.push(format!(
                    "No eligible reviewer remained for area '{}'.",
                    area.as_str()
                ));
            }
        } else {
            reasons.push(format!(
                "No reviewer pool configured for area '{}'.",
                area.as_str()
            ));
        }
    }

    if reviewers.is_empty() {
        if let Some(fallback) = first_non_excluded_and_unselected(
            &config.fallback_reviewers,
            excluded_reviewers,
            &selected,
        ) {
            reviewers.push(fallback.clone());
            reasons.push(
                "No area reviewer could be selected; fallback reviewer selected.".to_string(),
            );
        } else {
            reasons.push(
                "No eligible reviewer could be selected from configured areas or fallback pool."
                    .to_string(),
            );
        }
    }

    ReviewerRecommendation { reviewers, reasons }
}

pub fn recommend_reviewers_with_codeowners(
    summary: &MergeRequestAreaSummary,
    config: &ReviewerRoutingConfig,
    excluded_reviewers: &[String],
    codeowners_usernames: &[String],
) -> ReviewerRecommendation {
    let mut reviewers = Vec::new();
    let mut reasons = Vec::new();
    let mut selected = HashSet::new();

    for username in codeowners_usernames {
        if reviewers.len() >= config.max_reviewers {
            reasons.push(format!(
                "Reviewer selection reached configured limit of {}.",
                config.max_reviewers
            ));
            return ReviewerRecommendation { reviewers, reasons };
        }

        if excluded_reviewers
            .iter()
            .any(|excluded| excluded == username)
        {
            reasons.push(format!(
                "Skipped CODEOWNERS reviewer '{}' because they are excluded.",
                username
            ));
            continue;
        }

        if selected.insert(username.clone()) {
            reviewers.push(username.clone());
            reasons.push(format!("Selected reviewer '{}' from CODEOWNERS.", username));
        }
    }

    let fallback = recommend_reviewers(summary, config, excluded_reviewers);

    for reviewer in fallback.reviewers {
        if reviewers.len() >= config.max_reviewers {
            reasons.push(format!(
                "Reviewer selection reached configured limit of {}.",
                config.max_reviewers
            ));
            break;
        }

        if selected.insert(reviewer.clone()) {
            reasons.push(format!(
                "Selected reviewer '{}' from area routing fallback.",
                reviewer
            ));
            reviewers.push(reviewer);
        }
    }

    reasons.extend(fallback.reasons);

    ReviewerRecommendation { reviewers, reasons }
}

fn first_non_excluded<'a>(pool: &'a [String], excluded: &[String]) -> Option<&'a String> {
    pool.iter()
        .find(|candidate| !excluded.iter().any(|excluded| excluded == *candidate))
}

fn first_non_excluded_and_unselected<'a>(
    pool: &'a [String],
    excluded: &[String],
    selected: &HashSet<String>,
) -> Option<&'a String> {
    pool.iter().find(|candidate| {
        !excluded.iter().any(|excluded| excluded == *candidate) && !selected.contains(*candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::area_summary::MergeRequestAreaSummary;

    #[test]
    fn recommends_multiple_reviewers_for_highest_priority_areas() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);
        summary.add(CodeArea::Frontend);
        summary.add(CodeArea::Backend);
        summary.add(CodeArea::Backend);
        summary.add(CodeArea::Shared);

        let config = ReviewerRoutingConfig::example();
        let recommendation = recommend_reviewers(&summary, &config, &[]);

        assert_eq!(recommendation.reviewers.len(), 2);
        assert_eq!(recommendation.reviewers[0], "carol");
        assert_eq!(recommendation.reviewers[1], "alice");
    }

    #[test]
    fn skips_excluded_reviewer_and_uses_next_candidate() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);

        let config = ReviewerRoutingConfig::example();
        let excluded = vec!["alice".to_string()];
        let recommendation = recommend_reviewers(&summary, &config, &excluded);

        assert_eq!(recommendation.reviewers, vec!["bob".to_string()]);
    }

    #[test]
    fn deduplicates_selected_reviewers() {
        let mut config = ReviewerRoutingConfig::example();
        config
            .reviewers_by_area
            .insert(CodeArea::Frontend, vec!["alice".to_string()]);
        config
            .reviewers_by_area
            .insert(CodeArea::Documentation, vec!["alice".to_string()]);
        config.max_reviewers = 3;

        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);
        summary.add(CodeArea::Documentation);

        let recommendation = recommend_reviewers(&summary, &config, &[]);

        assert_eq!(recommendation.reviewers, vec!["alice".to_string()]);
    }

    #[test]
    fn falls_back_when_summary_is_empty() {
        let summary = MergeRequestAreaSummary::new();

        let config = ReviewerRoutingConfig::example();
        let recommendation = recommend_reviewers(&summary, &config, &[]);

        assert_eq!(recommendation.reviewers, vec!["milchick-duty".to_string()]);
    }

    #[test]
    fn respects_max_reviewer_limit() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);
        summary.add(CodeArea::Backend);
        summary.add(CodeArea::Shared);

        let mut config = ReviewerRoutingConfig::example();
        config.max_reviewers = 2;

        let recommendation = recommend_reviewers(&summary, &config, &[]);

        assert_eq!(recommendation.reviewers.len(), 2);
    }

    #[test]
    fn prefers_codeowners_reviewers_before_area_routing() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);

        let config = ReviewerRoutingConfig::example();
        let excluded = vec![];
        let codeowners = vec!["daniel.andrei".to_string()];

        let recommendation =
            recommend_reviewers_with_codeowners(&summary, &config, &excluded, &codeowners);

        assert_eq!(recommendation.reviewers[0], "daniel.andrei");
    }

    #[test]
    fn excludes_author_from_codeowners_candidates() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);

        let config = ReviewerRoutingConfig::example();
        let excluded = vec!["daniel.andrei".to_string()];
        let codeowners = vec!["daniel.andrei".to_string(), "andrei.achim".to_string()];

        let recommendation =
            recommend_reviewers_with_codeowners(&summary, &config, &excluded, &codeowners);

        assert_eq!(recommendation.reviewers[0], "andrei.achim");
    }

    #[test]
    fn falls_back_to_area_routing_when_codeowners_is_empty() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);

        let config = ReviewerRoutingConfig::example();

        let recommendation = recommend_reviewers_with_codeowners(&summary, &config, &[], &[]);

        assert_eq!(recommendation.reviewers[0], "alice");
    }
}
