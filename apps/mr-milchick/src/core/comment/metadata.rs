use serde::{Deserialize, Serialize};

use crate::core::model::ReviewActionKind;

const SUMMARY_METADATA_PREFIX: &str = "<!-- mr-milchick:summary-metadata ";
const SUMMARY_METADATA_SUFFIX: &str = " -->";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GovernanceExecutionStrategy {
    DryRun,
    Real,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceSummaryMetadata {
    pub execution_strategy: GovernanceExecutionStrategy,
    pub blocked: bool,
    pub applied_action_kinds: Vec<ReviewActionKind>,
    pub had_governance_effect: bool,
}

impl GovernanceSummaryMetadata {
    pub fn should_run_explain(&self) -> bool {
        self.blocked || self.had_governance_effect
    }
}

pub fn append_governance_summary_metadata(
    markdown: &str,
    metadata: &GovernanceSummaryMetadata,
) -> Result<String, serde_json::Error> {
    let payload = serde_json::to_string(metadata)?;
    let metadata_block = format!("{SUMMARY_METADATA_PREFIX}{payload}{SUMMARY_METADATA_SUFFIX}");

    Ok(if markdown.trim().is_empty() {
        metadata_block
    } else {
        format!("{}\n\n{}", markdown.trim(), metadata_block)
    })
}

pub fn parse_governance_summary_metadata(
    body: &str,
) -> Result<Option<GovernanceSummaryMetadata>, String> {
    let Some(start) = body.find(SUMMARY_METADATA_PREFIX) else {
        return Ok(None);
    };
    let payload = &body[start + SUMMARY_METADATA_PREFIX.len()..];
    let Some(end) = payload.find(SUMMARY_METADATA_SUFFIX) else {
        return Err("unclosed governance summary metadata block".to_string());
    };

    serde_json::from_str::<GovernanceSummaryMetadata>(&payload[..end])
        .map(Some)
        .map_err(|err| format!("invalid governance summary metadata: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governance_summary_metadata_round_trips() {
        let metadata = GovernanceSummaryMetadata {
            execution_strategy: GovernanceExecutionStrategy::Real,
            blocked: true,
            applied_action_kinds: vec![
                ReviewActionKind::AssignReviewers,
                ReviewActionKind::AddLabels,
            ],
            had_governance_effect: true,
        };

        let rendered = append_governance_summary_metadata("## Summary", &metadata)
            .expect("metadata should serialize");
        let parsed = parse_governance_summary_metadata(&rendered)
            .expect("metadata should parse")
            .expect("metadata should exist");

        assert_eq!(parsed, metadata);
    }

    #[test]
    fn parsing_returns_none_when_metadata_is_missing() {
        let parsed = parse_governance_summary_metadata("<!-- mr-milchick:summary -->")
            .expect("missing metadata should not error");

        assert!(parsed.is_none());
    }

    #[test]
    fn parsing_errors_for_malformed_payloads() {
        let error =
            parse_governance_summary_metadata("<!-- mr-milchick:summary-metadata {not-json} -->")
                .expect_err("malformed metadata should fail");

        assert!(error.contains("invalid governance summary metadata"));
    }

    #[test]
    fn explain_gate_requires_effect_or_blocking() {
        let metadata = GovernanceSummaryMetadata {
            execution_strategy: GovernanceExecutionStrategy::DryRun,
            blocked: false,
            applied_action_kinds: Vec::new(),
            had_governance_effect: false,
        };

        assert!(!metadata.should_run_explain());
        assert!(
            GovernanceSummaryMetadata {
                blocked: true,
                ..metadata.clone()
            }
            .should_run_explain()
        );
        assert!(
            GovernanceSummaryMetadata {
                had_governance_effect: true,
                ..metadata
            }
            .should_run_explain()
        );
    }
}
