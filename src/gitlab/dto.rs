use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MergeRequestDto {
    pub iid: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub draft: bool,
    pub web_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergeRequestChangesDto {
    pub changes: Vec<ChangedFileDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangedFileDto {
    pub old_path: String,
    pub new_path: String,
    pub new_file: bool,
    pub renamed_file: bool,
    pub deleted_file: bool,
}