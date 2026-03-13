#[derive(Debug, Clone, Copy)]
pub enum ToneMode {
    DeterministicMr,
    DeterministicPipeline,
    Random,
}

#[derive(Debug, Clone, Copy)]
pub struct ToneConfig {
    pub mode: ToneMode,
}

impl Default for ToneConfig {
    fn default() -> Self {
        Self {
            mode: ToneMode::DeterministicMr,
        }
    }
}