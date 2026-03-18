#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerRef {
    User(String),
    Group(String),
    Role(String),
}

impl OwnerRef {
    pub fn as_user(&self) -> Option<&str> {
        match self {
            OwnerRef::User(username) => Some(username.as_str()),
            OwnerRef::Group(_) | OwnerRef::Role(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersSection {
    pub id: String,
    pub name: String,
    pub required_approvals: usize,
    pub optional: bool,
    pub line_number: usize,
    pub default_owners: Vec<OwnerRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersRule {
    pub pattern: String,
    pub owners: Vec<OwnerRef>,
    pub line_number: usize,
    pub section_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersFile {
    pub sections: Vec<CodeownersSection>,
    pub rules: Vec<CodeownersRule>,
}

impl CodeownersFile {
    pub fn section_by_id(&self, section_id: &str) -> Option<&CodeownersSection> {
        self.sections
            .iter()
            .find(|section| section.id == section_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedSectionRequirement {
    pub section_id: String,
    pub section_name: String,
    pub required_approvals: usize,
    pub eligible_users: Vec<String>,
    pub matched_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageGap {
    pub section_name: String,
    pub required_approvals: usize,
    pub eligible_users: Vec<String>,
    pub reachable_approvals: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersAssignmentPlan {
    pub matched_sections: Vec<MatchedSectionRequirement>,
    pub assigned_reviewers: Vec<String>,
    pub uncovered_sections: Vec<CoverageGap>,
    pub reasons: Vec<String>,
}
