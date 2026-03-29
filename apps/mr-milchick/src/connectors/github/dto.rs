use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestDto {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub draft: bool,
    pub html_url: String,
    pub user: UserDto,
    #[serde(default)]
    pub requested_reviewers: Vec<UserDto>,
    #[serde(default)]
    pub labels: Vec<LabelDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserDto {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelDto {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestFileDto {
    pub filename: String,
    #[serde(default)]
    pub previous_filename: Option<String>,
    pub status: String,
    #[serde(default)]
    pub additions: Option<u32>,
    #[serde(default)]
    pub deletions: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueCommentDto {
    pub id: u64,
    pub body: String,
}
