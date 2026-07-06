//! Config generation (implemented by the generator work package — see the
//! interface contract in PLAN.md §4/§7).

pub mod lint;

use std::collections::BTreeMap;

/// Everything the generator needs; assembled by the API layer from DB rows
/// and settings. Filenames map 1:1 to /etc/angie/http.d entries.
pub struct GeneratorInput {
    pub hosts: Vec<crate::model::ProxyHost>,
    pub settings: EffectiveSettings,
    /// Read-only shared snippet files (block-exploits.conf, cache-assets.conf).
    pub snippets_dir: std::path::PathBuf,
    /// Where the status API server listens (127.0.0.1:<port>).
    pub status_port: u16,
    /// Directory served for the custom-HTML default site.
    pub public_dir: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct EffectiveSettings {
    pub default_site: DefaultSite,
    pub ipv6_enabled: bool,
    /// Nameservers for the `resolver` directive (from resolv.conf or override).
    pub resolvers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultSite {
    NotFound,
    Drop444,
    Redirect(String),
    Html,
}

/// filename → file body (WITHOUT the MANAGED-BY header; wrap with
/// `with_header` before writing to disk).
pub type FileSet = BTreeMap<String, String>;

pub fn generate(input: &GeneratorInput) -> anyhow::Result<FileSet> {
    let _ = input;
    todo!("generator work package")
}

/// Prepend the MANAGED-BY header (version + sha256 of the body).
pub fn with_header(body: &str) -> String {
    let _ = body;
    todo!("generator work package")
}

/// Parse a managed file: returns (declared_hash, actual_hash_matches).
pub struct ManagedMeta {
    pub generator_version: String,
    pub declared_hash: String,
    pub hash_matches: bool,
}

pub fn managed_meta(content: &str) -> Option<ManagedMeta> {
    let _ = content;
    todo!("generator work package")
}
