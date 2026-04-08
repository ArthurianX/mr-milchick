use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MergeRequestDto {
    pub iid: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub draft: bool,
    pub web_url: String,
    pub author: AuthorDto,
    pub reviewers: Vec<UserDto>,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorDto {
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserDto {
    pub username: String,
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
    #[serde(default)]
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserLookupDto {
    pub id: u64,
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergeRequestNoteDto {
    pub id: u64,
    pub body: String,
}
