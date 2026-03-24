use crate::actions::model::ActionPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingSeverity {
    Info,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFinding {
    pub severity: FindingSeverity,
    pub message: String,
}

impl RuleFinding {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            severity: FindingSeverity::Info,
            message: message.into(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: FindingSeverity::Warning,
            message: message.into(),
        }
    }

    pub fn blocking(message: impl Into<String>) -> Self {
        Self {
            severity: FindingSeverity::Blocking,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleOutcome {
    pub findings: Vec<RuleFinding>,
    pub action_plan: ActionPlan,
}

impl RuleOutcome {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
            action_plan: ActionPlan::new(),
        }
    }

    pub fn push(&mut self, finding: RuleFinding) {
        self.findings.push(finding);
    }

    pub fn has_blocking_findings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Blocking)
    }

    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }
}
