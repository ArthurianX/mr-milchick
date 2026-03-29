use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::core::context::model::CiContext;
use crate::core::tone::category::ToneCategory;
use crate::core::tone::registry::messages_for;

#[derive(Debug, Default)]
pub struct ToneSelector;

impl ToneSelector {
    pub fn select(&self, category: ToneCategory, ctx: &CiContext) -> &'static str {
        let messages = messages_for(category);
        let review_seed = ctx
            .review
            .as_ref()
            .map(|review| review.id.0.as_str())
            .unwrap_or("no-review");
        let index = stable_index(
            &format!("{}:{}:{:?}", ctx.project_key.0, review_seed, category),
            messages.len(),
        );

        messages[index]
    }
}

fn stable_index(seed: &str, len: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    (hasher.finish() as usize) % len
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::context::model::{
        BranchInfo, BranchName, CiContext, Label, PipelineInfo, PipelineSource, ProjectKey,
        ReviewContextRef, ReviewId,
    };

    fn sample_context() -> CiContext {
        CiContext {
            project_key: ProjectKey("123".to_string()),
            review: Some(ReviewContextRef {
                id: ReviewId("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::ReviewEvent,
            },
            branches: BranchInfo {
                source: BranchName("feat/test".to_string()),
                target: BranchName("develop".to_string()),
            },
            labels: vec![Label("backend".to_string())],
        }
    }

    #[test]
    fn selects_same_message_for_same_context() {
        let selector = ToneSelector::default();
        let ctx = sample_context();

        let first = selector.select(ToneCategory::Observation, &ctx);
        let second = selector.select(ToneCategory::Observation, &ctx);

        assert_eq!(first, second);
    }

    #[test]
    fn supports_missing_review() {
        let selector = ToneSelector::default();
        let mut ctx = sample_context();
        ctx.review = None;

        let msg = selector.select(ToneCategory::Observation, &ctx);

        assert!(!msg.is_empty());
    }
}
