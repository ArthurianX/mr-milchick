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