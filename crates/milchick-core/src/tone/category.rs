#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToneCategory {
    Observation,
    Refinement,
    Resolution,
    Blocking,
    Praise,
    ReviewRequest,
    NoAction,
    ReviewerAssigned,
}
