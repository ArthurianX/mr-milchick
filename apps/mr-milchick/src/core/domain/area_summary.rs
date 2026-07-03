use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MergeRequestAreaSummary {
    pub counts: HashMap<String, usize>,
    pub unmatched_paths: Vec<String>,
}

impl MergeRequestAreaSummary {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
            unmatched_paths: Vec::new(),
        }
    }

    pub fn add(&mut self, area: impl Into<String>) {
        let area = area.into();
        *self.counts.entry(area).or_insert(0) += 1;
    }

    pub fn add_unmatched(&mut self, path: impl Into<String>) {
        self.unmatched_paths.push(path.into());
    }

    pub fn dominant_area(&self) -> Option<String> {
        self.counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(area, _)| area.clone())
    }

    #[cfg(test)]
    pub fn total_files(&self) -> usize {
        self.counts.values().sum()
    }

    pub fn significant_areas(&self) -> Vec<String> {
        let mut pairs: Vec<(String, usize)> = self
            .counts
            .iter()
            .map(|(area, count)| (area.clone(), *count))
            .collect();

        pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        pairs.into_iter().map(|(area, _)| area).collect()
    }
}
