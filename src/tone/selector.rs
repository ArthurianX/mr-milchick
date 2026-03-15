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