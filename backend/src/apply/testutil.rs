//! Test-only helpers. The generator's `with_header`/`generate` are `todo!()`
//! stubs on this branch, so tests never call them; instead we synthesize
//! header-wrapped bodies here, matching the documented format
//! `# MANAGED BY angie-panel <ver> sha256:<hex>` (hash over the body only).

use sha2::{Digest, Sha256};

/// Wrap `body` with a valid MANAGED-BY header (as the generator would).
pub fn managed_body(body: &str) -> String {
    let body = if body.ends_with('\n') {
        body.to_string()
    } else {
        format!("{body}\n")
    };
    let mut h = Sha256::new();
    h.update(body.as_bytes());
    let hash = hex::encode(h.finalize());
    format!("# MANAGED BY angie-panel 0.1.0 sha256:{hash}\n{body}")
}

/// A plain foreign file (no MANAGED-BY header) — the apply pipeline must leave
/// these alone.
pub fn foreign_body(body: &str) -> String {
    format!("# hand-written by the operator\n{body}\n")
}

/// Build a header-wrapped [`crate::generator::FileSet`] from `(name, body)`
/// pairs. Bodies are wrapped as the generator would before handing the set to
/// `stage()`.
pub fn managed_fileset<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> crate::generator::FileSet {
    files
        .into_iter()
        .map(|(n, b)| (n.to_string(), managed_body(b)))
        .collect()
}
