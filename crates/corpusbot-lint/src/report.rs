use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LintIssue {
    pub code: String,
    pub severity: Severity,
    pub path: String,
    pub message: String,
    pub fix_hint: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LintSummary {
    pub pages: usize,
    pub errors: usize,
    pub warnings: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct LintReport {
    pub generated_at: String,
    pub template: String,
    pub revision_manifest_id: String,
    pub summary: LintSummary,
    pub issues: Vec<LintIssue>,
}
