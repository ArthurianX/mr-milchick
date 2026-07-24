use super::{LocalPromptStyle, ReviewSnapshot, build_local_inference_prompt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReviewPromptStyle {
    Primary,
    StrictRetry,
}

pub(crate) fn build_review_prompt(
    snapshot: &ReviewSnapshot,
    max_patch_bytes: usize,
    style: ReviewPromptStyle,
) -> String {
    build_local_inference_prompt(
        snapshot,
        max_patch_bytes,
        match style {
            ReviewPromptStyle::Primary => LocalPromptStyle::Primary,
            ReviewPromptStyle::StrictRetry => LocalPromptStyle::StrictRetry,
        },
    )
}
