//! Directive-allowlist linter over *generated* files (trust boundary,
//! PLAN.md §7 level 2). Implemented by the generator work package.

pub struct LintPolicy {
    pub snippets_dir: std::path::PathBuf,
    pub public_dir: std::path::PathBuf,
    pub allow_advanced_snippets: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LintViolation {
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
}

pub fn check_fileset(
    files: &crate::generator::FileSet,
    policy: &LintPolicy,
) -> Vec<LintViolation> {
    let _ = (files, policy);
    todo!("generator work package")
}
