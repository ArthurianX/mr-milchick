use std::collections::HashMap;

use crate::domain::area_summary::MergeRequestAreaSummary;
use crate::domain::code_area::CodeArea;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerRoutingConfig {
    pub reviewers_by_area: HashMap<CodeArea, Vec<String>>,
    pub fallback_reviewers: Vec<String>,
}

impl ReviewerRoutingConfig {
    pub fn example() -> Self {
        let mut reviewers_by_area = HashMap::new();

        reviewers_by_area.insert(
            CodeArea::Frontend,
            vec!["alice".to_string(), "bob".to_string()],
        );
        reviewers_by_area.insert(
            CodeArea::Backend,
            vec!["carol".to_string(), "dave".to_string()],
        );
        reviewers_by_area.insert(
            CodeArea::Shared,
            vec!["erin".to_string(), "frank".to_string()],
        );
        reviewers_by_area.insert(CodeArea::DevOps, vec!["grace".to_string()]);
        reviewers_by_area.insert(CodeArea::Documentation, vec!["heidi".to_string()]);
        reviewers_by_area.insert(CodeArea::Tests, vec!["ivan".to_string()]);

        Self {
            reviewers_by_area,
            fallback_reviewers: vec!["milchick-duty".to_string()],
        }
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
) -> ReviewerRecommendation {
    let mut reviewers = Vec::new();
    let mut reasons = Vec::new();

    if let Some(dominant_area) = summary.dominant_area() {
        if let Some(pool) = config.reviewers_by_area.get(&dominant_area) {
            if let Some(first) = pool.first() {
                reviewers.push(first.clone());
                reasons.push(format!(
                    "Selected reviewer '{}' for dominant area '{}'.",
                    first,
                    dominant_area.as_str()
                ));
            }
        } else if let Some(first) = config.fallback_reviewers.first() {
            reviewers.push(first.clone());
            reasons.push(format!(
                "No reviewer pool configured for dominant area '{}'; fallback reviewer selected.",
                dominant_area.as_str()
            ));
        }
    } else if let Some(first) = config.fallback_reviewers.first() {
        reviewers.push(first.clone());
        reasons.push("No dominant area detected; fallback reviewer selected.".to_string());
    }

    ReviewerRecommendation { reviewers, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::area_summary::MergeRequestAreaSummary;

    #[test]
    fn recommends_reviewer_for_dominant_area() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Frontend);
        summary.add(CodeArea::Frontend);
        summary.add(CodeArea::Backend);

        let config = ReviewerRoutingConfig::example();
        let recommendation = recommend_reviewers(&summary, &config);

        assert_eq!(recommendation.reviewers, vec!["alice".to_string()]);
        assert_eq!(recommendation.reasons.len(), 1);
    }

    #[test]
    fn falls_back_when_no_area_mapping_exists() {
        let mut summary = MergeRequestAreaSummary::new();
        summary.add(CodeArea::Unknown);

        let config = ReviewerRoutingConfig::example();
        let recommendation = recommend_reviewers(&summary, &config);

        assert_eq!(
            recommendation.reviewers,
            vec!["milchick-duty".to_string()]
        );
        assert_eq!(recommendation.reasons.len(), 1);
    }

    #[test]
    fn falls_back_when_summary_is_empty() {
        let summary = MergeRequestAreaSummary::new();

        let config = ReviewerRoutingConfig::example();
        let recommendation = recommend_reviewers(&summary, &config);

        assert_eq!(
            recommendation.reviewers,
            vec!["milchick-duty".to_string()]
        );
    }
}