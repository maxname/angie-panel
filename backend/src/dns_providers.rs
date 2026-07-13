//! DNS-01 provider registry for automatic wildcard issuance. Each provider maps
//! to an acme.sh dnsapi plugin (`dnsapi/dns_<plugin>.sh`) and the credential
//! environment variables that plugin reads. The ACME hook exports the operator's
//! stored credentials as those env vars, sources the plugin (after acme.sh's
//! core helpers), and calls `dns_<plugin>_add`/`_rm`. Verified on real Angie +
//! pebble; env var names checked against the actual acme.sh plugins.
//!
//! To add a provider, append a row here (id + acme.sh plugin + its credential
//! env vars). The rest of the stack (storage, hook, UI) is data-driven from this.

/// One credential field a provider needs, and the acme.sh env var it maps to.
pub struct CredField {
    /// acme.sh environment variable name (e.g. "CF_Token"). Also the storage key
    /// suffix, so it is stable.
    pub env: &'static str,
    /// Human label for the field in the UI.
    pub label: &'static str,
}

pub struct ProviderDef {
    /// Stable id stored on the certificate (e.g. "cloudflare").
    pub id: &'static str,
    /// Display name.
    pub label: &'static str,
    /// acme.sh plugin base name → `dnsapi/dns_<plugin>.sh` + `dns_<plugin>_add`.
    pub plugin: &'static str,
    /// Credential fields, in UI order. All are treated as secrets.
    pub fields: &'static [CredField],
}

macro_rules! f {
    ($env:literal, $label:literal) => {
        CredField {
            env: $env,
            label: $label,
        }
    };
}

/// Curated set — the most common providers, env vars verified against acme.sh.
/// Extend freely; the plugin must exist in the bundled dnsapi dir.
pub static PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "cloudflare",
        label: "Cloudflare",
        plugin: "cf",
        fields: &[f!("CF_Token", "API token")],
    },
    ProviderDef {
        id: "route53",
        label: "AWS Route 53",
        plugin: "aws",
        fields: &[
            f!("AWS_ACCESS_KEY_ID", "Access key ID"),
            f!("AWS_SECRET_ACCESS_KEY", "Secret access key"),
        ],
    },
    ProviderDef {
        id: "digitalocean",
        label: "DigitalOcean",
        plugin: "dgon",
        fields: &[f!("DO_API_KEY", "API key")],
    },
    ProviderDef {
        id: "gandi",
        label: "Gandi LiveDNS",
        plugin: "gandi_livedns",
        fields: &[f!("GANDI_LIVEDNS_KEY", "API key")],
    },
    ProviderDef {
        id: "desec",
        label: "deSEC",
        plugin: "desec",
        fields: &[f!("DEDYN_TOKEN", "Token")],
    },
    ProviderDef {
        id: "namecheap",
        label: "Namecheap",
        plugin: "namecheap",
        fields: &[
            f!("NAMECHEAP_USERNAME", "Username"),
            f!("NAMECHEAP_API_KEY", "API key"),
            f!("NAMECHEAP_SOURCEIP", "Whitelisted source IP"),
        ],
    },
    ProviderDef {
        id: "godaddy",
        label: "GoDaddy",
        plugin: "gd",
        fields: &[f!("GD_Key", "Key"), f!("GD_Secret", "Secret")],
    },
    ProviderDef {
        id: "vultr",
        label: "Vultr",
        plugin: "vultr",
        fields: &[f!("VULTR_API_KEY", "API key")],
    },
    ProviderDef {
        id: "linode",
        label: "Linode",
        plugin: "linode_v4",
        fields: &[f!("LINODE_V4_API_KEY", "API token")],
    },
    ProviderDef {
        id: "porkbun",
        label: "Porkbun",
        plugin: "porkbun",
        fields: &[
            f!("PORKBUN_API_KEY", "API key"),
            f!("PORKBUN_SECRET_API_KEY", "Secret API key"),
        ],
    },
    ProviderDef {
        id: "regru",
        label: "reg.ru",
        plugin: "regru",
        fields: &[
            f!("REGRU_API_Username", "API username"),
            f!("REGRU_API_Password", "API password"),
        ],
    },
];

/// Look up a provider by its stored id.
pub fn get(id: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Whether `id` is a known provider.
pub fn is_valid(id: &str) -> bool {
    get(id).is_some()
}

/// Settings key holding one provider credential: `dns_cred:<provider>:<ENV_VAR>`.
/// These are secrets — redacted from the settings GET and never exported.
pub fn cred_key(provider: &str, env: &str) -> String {
    format!("dns_cred:{provider}:{env}")
}

/// True for any settings key that stores a DNS provider credential.
pub fn is_cred_key(key: &str) -> bool {
    key.starts_with("dns_cred:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        for p in PROVIDERS {
            assert!(!p.fields.is_empty(), "{} has no credential fields", p.id);
            assert!(get(p.id).is_some());
            assert!(is_valid(p.id));
            // ids unique
            assert_eq!(
                PROVIDERS.iter().filter(|q| q.id == p.id).count(),
                1,
                "duplicate id {}",
                p.id
            );
        }
        assert!(!is_valid("nope"));
        assert!(get("nope").is_none());
    }

    #[test]
    fn cred_key_shape() {
        assert_eq!(
            cred_key("cloudflare", "CF_Token"),
            "dns_cred:cloudflare:CF_Token"
        );
        assert!(is_cred_key("dns_cred:cloudflare:CF_Token"));
        assert!(!is_cred_key("acme_email"));
    }

    #[test]
    fn regru_matches_acmesh_env_vars() {
        // Locked to the acme.sh dns_regru contract (verified against the plugin).
        let p = get("regru").unwrap();
        assert_eq!(p.plugin, "regru");
        let envs: Vec<_> = p.fields.iter().map(|f| f.env).collect();
        assert!(envs.contains(&"REGRU_API_Username"));
        assert!(envs.contains(&"REGRU_API_Password"));
    }
}
