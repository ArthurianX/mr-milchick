use crate::core::domain::area_summary::MergeRequestAreaSummary;
use crate::core::domain::path_classifier::classify_path;
use crate::core::model::{AreasConfig, ReviewSnapshot};

pub fn summarize_areas(snapshot: &ReviewSnapshot, areas: &AreasConfig) -> MergeRequestAreaSummary {
    let mut summary = MergeRequestAreaSummary::new();

    for file in &snapshot.changed_files {
        if let Some(area) = classify_path(&file.path, areas) {
            summary.add(area);
        } else {
            summary.add_unmatched(file.path.clone());
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
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
                    previous_path: None,
                    change_type: ChangeType::Modified,
                    additions: None,
                    deletions: None,
                    patch: None,
                },
                ChangedFile {
                    path: "services/api/main.rs".into(),
                    previous_path: None,
                    change_type: ChangeType::Modified,
                    additions: None,
                    deletions: None,
                    patch: None,
                },
            ],
        }
    }

    #[test]
    fn builds_area_summary() {
        let areas = AreasConfig {
            definitions: vec![
                crate::core::model::AreaDefinition {
                    key: "frontend".into(),
                    paths: vec!["apps/frontend/**".into()],
                    risk: crate::core::model::AreaRisk::Medium,
                    critical: false,
                },
                crate::core::model::AreaDefinition {
                    key: "backend".into(),
                    paths: vec!["services/**".into()],
                    risk: crate::core::model::AreaRisk::Medium,
                    critical: false,
                },
            ],
        };

        let summary = summarize_areas(&sample_snapshot(), &areas);

        assert_eq!(summary.total_files(), 2);
        assert!(summary.dominant_area().is_some());
    }
}
