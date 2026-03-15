use std::collections::HashMap;

use crate::domain::code_area::CodeArea;

#[derive(Debug, Clone)]
pub struct MergeRequestAreaSummary {
    pub counts: HashMap<CodeArea, usize>,
}

impl MergeRequestAreaSummary {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    pub fn add(&mut self, area: CodeArea) {
        *self.counts.entry(area).or_insert(0) += 1;
    }

    pub fn dominant_area(&self) -> Option<CodeArea> {
        self.counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(area, _)| *area)
    }

    pub fn total_files(&self) -> usize {
        self.counts.values().sum()
    }
}