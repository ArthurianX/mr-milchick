#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersRule {
    pub pattern: String,
    pub owners: Vec<String>,
    pub line_number: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersFile {
    pub rules: Vec<CodeownersRule>,
}

impl CodeownersFile {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
