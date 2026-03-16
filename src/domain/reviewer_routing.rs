use std::collections::{HashMap, HashSet};

use crate::config::model::MrMilchickConfig;
use crate::domain::area_summary::MergeRequestAreaSummary;
use crate::domain::code_area::CodeArea;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerRoutingConfig {
    pub reviewers_by_area: HashMap<CodeArea, Vec<String>>,
    pub fallback_reviewers: Vec<String>,
    pub max_reviewers: usize,
}

impl ReviewerRoutingConfig {
    pub fn from_config(config: &MrMilchickConfig) -> Self {
        let mut reviewers_by_area = HashMap::new();

        reviewers_by_area.insert(CodeArea::Frontend, config.reviewers.frontend.clone());
        reviewers_by_area.insert(CodeArea::Backend, config.reviewers.backend.clone());
        reviewers_by_area.insert(CodeArea::Shared, config.reviewers.shared.clone());
        reviewers_by_area.insert(CodeArea::DevOps, config.reviewers.devops.clone());
        reviewers_by_area.insert(
            CodeArea::Documentation,
            config.reviewers.documentation.clone(),
        );
        reviewers_by_area.insert(CodeArea::Tests, config.reviewers.tests.clone());

        Self {
            reviewers_by_area,
            fallback_reviewers: config.reviewers.fallback_reviewers.clone(),
            max_reviewers: config.reviewers.max_reviewers,
        }
    }

    pub fn example() -> Self {
        let raw = MrMilchickConfig {
            reviewers: crate::config::model::ReviewerConfig {
                max_reviewers: 2,
                fallback_reviewers: vec!["milchick-duty".to_string()],
                frontend: vec!["alice".to_string(), "bob".to_string()],
                backend: vec!["carol".to_string(), "dave".to_string()],
                shared: vec!["erin".to_string(), "frank".to_string()],
                devops: vec!["grace".to_string()],
                documentation: vec!["heidi".to_string()],
                tests: vec!["ivan".to_string()],
            },
            codeowners: None,
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
            reasons.push("No dominant area detected and no eligible fallback reviewer exists.".to_string());
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
            if let Some(candidate) = first_non_excluded_and_unselected(
                pool,
                excluded_reviewers,
                &selected,
            ) {
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
            reasons.push("No area reviewer could be selected; fallback reviewer selected.".to_string());
        } else {
            reasons.push("No eligible reviewer could be selected from configured areas or fallback pool.".to_string());
        }
    }

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
}