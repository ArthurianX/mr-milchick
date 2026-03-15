use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::context::model::CiContext;
use crate::tone::category::ToneCategory;
use crate::tone::config::{ToneConfig, ToneMode};
use crate::tone::registry::messages_for;

#[derive(Debug, Default)]
pub struct ToneSelector {
    config: ToneConfig,
}

impl ToneSelector {
    pub fn new(config: ToneConfig) -> Self {
        Self { config }
    }

    pub fn select(&self, category: ToneCategory, ctx: &CiContext) -> &'static str {
        let messages = messages_for(category);

        let index = match self.config.mode {
            ToneMode::DeterministicMr => {
                let mr_seed = ctx
                    .merge_request
                    .as_ref()
                    .map(|m| m.iid.0.as_str())
                    .unwrap_or("no-mr");

                stable_index(
                    &format!("{}:{}:{:?}", ctx.project_id.0, mr_seed, category),
                    messages.len(),
                )
            }

            ToneMode::DeterministicPipeline => {
                stable_index(
                    &format!(
                        "{}:{}:{:?}",
                        ctx.project_id.0,
                        ctx.pipeline.source as u8,
                        category
                    ),
                    messages.len(),
                )
            }

            ToneMode::Random => {
                // Placeholder: real RNG later
                stable_index(
                    &format!("fallback-random:{:?}", category),
                    messages.len(),
                )
            }
        };

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
    use crate::context::model::{
        BranchInfo, BranchName, CiContext, Label, MergeRequestIid, MergeRequestRef, PipelineInfo,
        PipelineSource, ProjectId,
    };

    fn sample_context() -> CiContext {
        CiContext {
            project_id: ProjectId("123".to_string()),
            merge_request: Some(MergeRequestRef {
                iid: MergeRequestIid("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::MergeRequestEvent,
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
    fn supports_missing_merge_request() {
        let selector = ToneSelector::default();
        let mut ctx = sample_context();
        ctx.merge_request = None;

        let msg = selector.select(ToneCategory::Observation, &ctx);

        assert!(!msg.is_empty());
    }
}