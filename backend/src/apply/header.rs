//! Parsing of the MANAGED-BY header that the generator stamps on every file it
//! produces (PLAN.md §2.2 "detect edits made outside the panel").
//!
//! The apply pipeline only ever *reads* this header — its input FileSets arrive
//! already header-wrapped by `generator::with_header`. We keep the parser here,
//! rather than call `generator::managed_meta`, so the staging/diff/manifest
//! side stays self-contained and unit-testable without the generator crate.
//!
//! FORMAT CONTRACT (must stay byte-compatible with `generator::with_header`):
//! the first line is
//!
//! ```text
//! # MANAGED BY angie-panel <ver> sha256:<hex>
//! ```
//!
//! where `<hex>` is the lowercase sha256 of everything **after** that first
//! line (the body). PLAN.md §2.2 also shows the abbreviated form `hash:<hex>`
//! and a `v<ver>` version token, so the parser accepts `sha256:`/`hash:` and an
//! optional leading `v` on the version — see `parse` below. INTEGRATION NOTE:
//! if the generator settles on a different token, update `PREFIX` here to match.

use sha2::{Digest, Sha256};

const PREFIX: &str = "# MANAGED BY angie-panel ";

/// Parsed MANAGED-BY header plus a body-integrity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedHeader {
    pub generator_version: String,
    /// Hash declared in the header line (lowercase hex).
    pub declared_hash: String,
    /// True when the declared hash equals sha256(body). False = the body was
    /// hand-edited after generation (drift).
    pub hash_matches: bool,
}

/// sha256 hex of the body (everything after the header line).
fn body_hash(body: &str) -> String {
    let mut h = Sha256::new();
    h.update(body.as_bytes());
    hex::encode(h.finalize())
}

/// Parse the MANAGED-BY header of a file. Returns `None` for a foreign file
/// (one that does not begin with our header prefix).
pub fn parse(content: &str) -> Option<ManagedHeader> {
    let (first_line, rest) = match content.split_once('\n') {
        Some((l, r)) => (l, r),
        // A file that is *only* the header line (no trailing newline/body).
        None => (content, ""),
    };
    let meta = first_line.strip_prefix(PREFIX)?.trim();

    // Remaining tokens: "<ver> sha256:<hex>" (or hash:<hex>).
    let mut ver = None;
    let mut declared = None;
    for tok in meta.split_whitespace() {
        if let Some(h) = tok
            .strip_prefix("sha256:")
            .or_else(|| tok.strip_prefix("hash:"))
        {
            declared = Some(h.to_ascii_lowercase());
        } else if ver.is_none() {
            ver = Some(tok.trim_start_matches('v').to_string());
        }
    }
    let declared_hash = declared?;
    Some(ManagedHeader {
        generator_version: ver.unwrap_or_default(),
        hash_matches: body_hash(rest) == declared_hash,
        declared_hash,
    })
}

/// Whether a file carries our header at all (managed vs. foreign).
pub fn is_managed(content: &str) -> bool {
    content.starts_with(PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::testutil::{foreign_body, managed_body};

    #[test]
    fn parses_intact_managed_file() {
        let m = parse(&managed_body("hello world")).unwrap();
        assert!(m.hash_matches);
        assert_eq!(m.generator_version, "0.1.0");
    }

    #[test]
    fn detects_body_drift() {
        // Header stamped for "original", body swapped for "tampered".
        let good = managed_body("original");
        let header_line = good.split_once('\n').unwrap().0;
        let tampered = format!("{header_line}\ntampered body\n");
        let m = parse(&tampered).unwrap();
        assert!(!m.hash_matches, "hand-edited body must fail the hash check");
    }

    #[test]
    fn foreign_file_is_not_managed() {
        assert!(parse(&foreign_body("something")).is_none());
        assert!(!is_managed(&foreign_body("x")));
        assert!(is_managed(&managed_body("x")));
    }

    #[test]
    fn accepts_hash_prefix_variant() {
        let body = "line1\nline2\n";
        let h = body_hash(body);
        let file = format!("# MANAGED BY angie-panel v0.1.0 hash:{h}\n{body}");
        let m = parse(&file).unwrap();
        assert!(m.hash_matches);
        assert_eq!(m.generator_version, "0.1.0");
    }
}
