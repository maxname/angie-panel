//! ACME certificate authority registry. A certificate names its CA by id; the
//! settings layer resolves that id to a directory URL (plus EAB credentials when
//! the CA needs them) and hands the generator a ready `acme_client` line.
//!
//! `custom` is the escape hatch for any other RFC 8555 CA: its directory URL is
//! a global setting rather than a registry constant.
//!
//! To add a CA, append a row here. Storage, validation and the UI are driven
//! from this table.

/// Whether a CA wants External Account Binding when an account is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Eab {
    /// The CA never asks for EAB (Let's Encrypt).
    None,
    /// Registration fails without EAB (ZeroSSL, Google Trust Services).
    Required,
    /// Depends on the CA behind the URL — only the custom entry.
    Optional,
}

pub struct CaDef {
    /// Stable id stored on the certificate.
    pub id: &'static str,
    /// Display name.
    pub label: &'static str,
    /// Production directory URL. Empty for `custom` (comes from settings).
    pub directory: &'static str,
    /// Staging directory, when the CA offers one reachable without separate
    /// credentials. `None` disables the per-certificate staging flag.
    pub staging_directory: Option<&'static str>,
    pub eab: Eab,
}

pub const DEFAULT_CA: &str = "letsencrypt";
pub const CUSTOM_CA: &str = "custom";

pub static CAS: &[CaDef] = &[
    CaDef {
        id: "letsencrypt",
        label: "Let's Encrypt",
        directory: crate::generator::LE_PROD_DIRECTORY,
        staging_directory: Some(crate::generator::LE_STAGING_DIRECTORY),
        eab: Eab::None,
    },
    CaDef {
        id: "zerossl",
        label: "ZeroSSL",
        directory: "https://acme.zerossl.com/v2/DV90",
        staging_directory: None,
        eab: Eab::Required,
    },
    CaDef {
        id: "google",
        label: "Google Trust Services",
        directory: "https://dv.acme-v02.api.pki.goog/directory",
        // Google's staging environment needs its own EAB keys from a separate
        // API call, so a shared staging toggle would only ever fail there.
        staging_directory: None,
        eab: Eab::Required,
    },
    CaDef {
        id: CUSTOM_CA,
        label: "Custom ACME server",
        directory: "",
        staging_directory: None,
        eab: Eab::Optional,
    },
];

/// Look up a CA by its stored id.
pub fn get(id: &str) -> Option<&'static CaDef> {
    CAS.iter().find(|c| c.id == id)
}

/// Settings key holding one half of a CA's EAB pair: `acme_eab:<ca>:kid|hmac`.
/// The HMAC key is a secret — sealed at rest, redacted from the settings GET,
/// never exported. The key id travels with it so the pair stays consistent.
pub fn eab_key(ca: &str, part: &str) -> String {
    format!("acme_eab:{ca}:{part}")
}

/// True for any settings key that stores EAB material.
pub fn is_eab_key(key: &str) -> bool {
    key.starts_with("acme_eab:")
}

/// EAB key ids and HMAC keys are emitted into `acme_client … eab=<kid>:<hmac>`,
/// so both are held to the base64url alphabet (which is what CAs hand out).
/// That rules out whitespace, `;`, `:` and quotes — nothing that could split
/// the parameter or end the directive.
pub fn is_valid_eab_part(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 512
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'='))
}

/// A custom directory URL is emitted verbatim as the `acme_client` URI, so it
/// must be https and free of anything Angie's parser treats as structure.
pub fn is_valid_directory_url(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() > "https://".len()
        && url.len() <= 2000
        && !url.contains(|c: char| {
            c.is_whitespace()
                || c.is_control()
                || matches!(c, ';' | '{' | '}' | '"' | '\'' | '\\' | '$')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        assert!(get(DEFAULT_CA).is_some());
        assert!(get(CUSTOM_CA).is_some());
        for c in CAS {
            assert_eq!(
                CAS.iter().filter(|d| d.id == c.id).count(),
                1,
                "dup {}",
                c.id
            );
            if c.id != CUSTOM_CA {
                assert!(is_valid_directory_url(c.directory), "{}", c.id);
            }
            if let Some(s) = c.staging_directory {
                assert!(is_valid_directory_url(s), "{}", c.id);
            }
        }
        assert!(get("nope").is_none());
    }

    #[test]
    fn eab_parts_reject_config_metacharacters() {
        assert!(is_valid_eab_part("kid_ABC-123"));
        assert!(is_valid_eab_part("aGVsbG8td29ybGQ="));
        for evil in ["", "a b", "a;b", "a:b", "a\"b", "a}b", "a\nb"] {
            assert!(!is_valid_eab_part(evil), "{evil:?}");
        }
    }

    #[test]
    fn directory_url_must_be_plain_https() {
        assert!(is_valid_directory_url("https://acme.example.com/directory"));
        for evil in [
            "http://acme.example.com/directory",
            "https://",
            "https://a.example/x;",
            "https://a.example/x y",
            "https://a.example/$var",
            "https://a.example/{",
        ] {
            assert!(!is_valid_directory_url(evil), "{evil}");
        }
    }

    #[test]
    fn eab_key_shape() {
        assert_eq!(eab_key("zerossl", "hmac"), "acme_eab:zerossl:hmac");
        assert!(is_eab_key("acme_eab:zerossl:kid"));
        assert!(!is_eab_key("acme_email"));
    }
}
