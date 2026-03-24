use crate::domain::area_summary::MergeRequestAreaSummary;
use crate::domain::path_classifier::classify_path;
use crate::model::ReviewSnapshot;

pub fn summarize_areas(snapshot: &ReviewSnapshot) -> MergeRequestAreaSummary {
    let mut summary = MergeRequestAreaSummary::new();

    for file in &snapshot.changed_files {
        let area = classify_path(&file.path);
        summary.add(area);
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Actor, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewPlatformKind,
        ReviewRef, ReviewSnapshot,
    };

    fn sample_snapshot() -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".into(),
                review_id: "1".into(),
                web_url: None,
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace: "group".into(),
                name: "project".into(),
                web_url: None,
            },
            title: "Test".into(),
            description: None,
            author: Actor {
                username: "arthur".into(),
                display_name: None,
            },
            participants: vec![],
            labels: vec![],
            is_draft: false,
            default_branch: Some("develop".into()),
            metadata: ReviewMetadata::default(),
            changed_files: vec![
                ChangedFile {
                    path: "apps/frontend/button.tsx".into(),
                    change_type: ChangeType::Modified,
                    additions: None,
                    deletions: None,
                },
                ChangedFile {
                    path: "services/api/main.rs".into(),
                    change_type: ChangeType::Modified,
                    additions: None,
                    deletions: None,
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
