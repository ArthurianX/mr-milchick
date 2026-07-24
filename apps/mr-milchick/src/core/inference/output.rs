use super::{ReviewInferenceError, ReviewInsights, parse_local_inference_output};

pub(crate) fn parse_review_output(raw: &str) -> Result<ReviewInsights, ReviewInferenceError> {
    parse_local_inference_output(raw)
}
