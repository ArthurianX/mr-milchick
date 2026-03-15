use crate::domain::area_summary::MergeRequestAreaSummary;
use crate::domain::path_classifier::classify_path;
use crate::gitlab::api::MergeRequestSnapshot;

pub fn summarize_areas(snapshot: &MergeRequestSnapshot) -> MergeRequestAreaSummary {
    let mut summary = MergeRequestAreaSummary::new();

    for file in &snapshot.changed_files {
        let area = classify_path(&file.new_path);
        summary.add(area);
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitlab::api::{ChangedFile, MergeRequestDetails, MergeRequestSnapshot, MergeRequestState};

    fn sample_snapshot() -> MergeRequestSnapshot {
        MergeRequestSnapshot {
            details: MergeRequestDetails {
                iid: 1,
                title: "Test".into(),
                description: None,
                state: MergeRequestState::Opened,
                is_draft: false,
                web_url: "".into(),
            },
            changed_files: vec![
                ChangedFile {
                    old_path: "".into(),
                    new_path: "apps/frontend/button.tsx".into(),
                    is_new: false,
                    is_renamed: false,
                    is_deleted: false,
                },
                ChangedFile {
                    old_path: "".into(),
                    new_path: "services/api/main.rs".into(),
                    is_new: false,
                    is_renamed: false,
                    is_deleted: false,
                },
            ],
        }
    }

    #[test]
    fn builds_area_summary() {
        let summary = summarize_areas(&sample_snapshot());

        assert_eq!(summary.total_files(), 2);
        assert!(summary.dominant_area().is_some());
    }
}