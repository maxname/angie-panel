//! Minimal reg.ru API client for DNS-01 TXT records, used by the ACME hook to
//! automate wildcard issuance (`zone/add_txt` on "add", `zone/remove_record`
//! on "remove"). Same endpoints the `acme.sh dns_regru` plugin uses. Credentials
//! are the reg.ru account (or API) username/password; they never leave the panel
//! except in these calls to reg.ru's own API.

use anyhow::{bail, Context};
use serde_json::Value;

/// Production reg.ru API base. Overridable so tests can point at a mock.
pub const REGRU_API_BASE: &str = "https://api.reg.ru/api/regru2";

pub struct RegruCreds {
    pub username: String,
    pub password: String,
}

/// Split an ACME domain into the reg.ru zone (`domain_name`) and the
/// `_acme-challenge` record's `subdomain` relative to that zone. The zone is the
/// last two labels (covers the common `.ru`/`.com`/`.рф` cases; multi-label
/// public suffixes like `.co.uk` are not auto-detected — documented).
pub fn acme_record(domain: &str) -> (String, String) {
    let domain = domain.strip_prefix("*.").unwrap_or(domain);
    let labels: Vec<&str> = domain.split('.').filter(|l| !l.is_empty()).collect();
    let (zone, prefix): (String, &[&str]) = if labels.len() >= 2 {
        let split = labels.len() - 2;
        (labels[split..].join("."), &labels[..split])
    } else {
        (domain.to_string(), &[])
    };
    let mut sub = String::from("_acme-challenge");
    if !prefix.is_empty() {
        sub.push('.');
        sub.push_str(&prefix.join("."));
    }
    (zone, sub)
}

async fn call(
    client: &reqwest::Client,
    base: &str,
    method: &str,
    params: &[(&str, &str)],
) -> anyhow::Result<()> {
    let url = format!("{}/{method}", base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .form(params)
        .send()
        .await
        .with_context(|| format!("reg.ru {method} request failed"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("reg.ru {method} HTTP {status}: {}", body.trim());
    }
    // reg.ru wraps the outcome in {"result":"success"|"error", ...}.
    let json: Value = serde_json::from_str(&body)
        .with_context(|| format!("reg.ru {method} returned non-JSON: {}", body.trim()))?;
    match json.get("result").and_then(Value::as_str) {
        Some("success") => Ok(()),
        _ => {
            let msg = json
                .get("error_text")
                .and_then(Value::as_str)
                .unwrap_or_else(|| body.trim());
            bail!("reg.ru {method} error: {msg}");
        }
    }
}

/// Create the `_acme-challenge` TXT record for `domain` with `value`.
pub async fn add_txt(
    client: &reqwest::Client,
    base: &str,
    creds: &RegruCreds,
    domain: &str,
    value: &str,
) -> anyhow::Result<()> {
    let (zone, sub) = acme_record(domain);
    call(
        client,
        base,
        "zone/add_txt",
        &[
            ("username", &creds.username),
            ("password", &creds.password),
            ("domain_name", &zone),
            ("subdomain", &sub),
            ("text", value),
            ("output_content_type", "plain"),
        ],
    )
    .await
}

/// Delete the `_acme-challenge` TXT record for `domain` with `value`.
pub async fn remove_txt(
    client: &reqwest::Client,
    base: &str,
    creds: &RegruCreds,
    domain: &str,
    value: &str,
) -> anyhow::Result<()> {
    let (zone, sub) = acme_record(domain);
    call(
        client,
        base,
        "zone/remove_record",
        &[
            ("username", &creds.username),
            ("password", &creds.password),
            ("domain_name", &zone),
            ("subdomain", &sub),
            ("record_type", "TXT"),
            ("content", value),
            ("output_content_type", "plain"),
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_split() {
        assert_eq!(
            acme_record("example.com"),
            ("example.com".into(), "_acme-challenge".into())
        );
        assert_eq!(
            acme_record("*.example.com"),
            ("example.com".into(), "_acme-challenge".into())
        );
        assert_eq!(
            acme_record("sub.example.ru"),
            ("example.ru".into(), "_acme-challenge.sub".into())
        );
        assert_eq!(
            acme_record("*.a.b.example.com"),
            ("example.com".into(), "_acme-challenge.a.b".into())
        );
    }
}
