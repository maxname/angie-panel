//! Directive-allowlist linter over *generated* files (PLAN.md §7 level 2).
//!
//! This is the real trust boundary. `model.rs` (level 1) reduces every
//! user-controlled field to a safe charset, and the generator interpolates
//! those fields into a fixed structure — but advanced snippets are inserted
//! *verbatim*, and a bug (here or upstream) could in principle let a hostile
//! value through. So the root helper re-checks the *final bytes* against a
//! hard deny-list before `angie -t` ever touches them. Writing to `http.d`
//! is root-equivalent (Angie's master runs `error_log`/`root`/`proxy_pass
//! unix:` as root), which is exactly what this linter prevents.
//!
//! Design: a deny-list, not a full parser. We do not need to *understand* the
//! config — we need to guarantee that none of a small set of dangerous
//! constructs appears anywhere, including inside a snippet that tries to break
//! out of its context with a stray `}`. So we tokenize just enough (strip
//! comments and quoted strings, then split into `;`/`{`/`}`-delimited
//! statements) to inspect the leading directive of every statement, wherever a
//! snippet placed it.

use std::net::IpAddr;
use std::path::Path;

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

/// Management ports a `proxy_pass` must never target: the Angie status API and
/// the panel itself. Loopback in general is covered by the IP check below, but
/// these are called out for a clearer message and to cover the hostname case.
const STATUS_PORT: u16 = 8100;
const PANEL_PORT: u16 = 8080;

/// A single tokenized statement: its leading word (the directive), the full
/// argument text, and the 1-based source line where it began.
struct Statement {
    directive: String,
    args: String,
    line: usize,
}

pub fn check_fileset(files: &crate::generator::FileSet, policy: &LintPolicy) -> Vec<LintViolation> {
    let mut violations = Vec::new();
    for (name, body) in files {
        // Only http.d *.conf files are checked here. Skip: non-.conf managed
        // data (e.g. `access-<id>.htpasswd`), and stream.d configs (a separate
        // context whose directives the http-oriented deny-list would
        // mis-analyze — stream values are strictly model-validated with no user
        // free-text, so there is nothing to allow-list at the output stage).
        if !name.ends_with(".conf") || name.starts_with(crate::generator::STREAM_PREFIX) {
            continue;
        }
        check_file(name, body, policy, &mut violations);
    }
    violations
}

fn check_file(name: &str, body: &str, policy: &LintPolicy, out: &mut Vec<LintViolation>) {
    let push = |out: &mut Vec<LintViolation>, line: usize, msg: String| {
        out.push(LintViolation {
            file: name.to_string(),
            line: Some(line),
            message: msg,
        });
    };

    // A tokenizer failure (unterminated quote, etc.) is itself suspicious —
    // report it rather than silently skipping the file.
    let statements = match tokenize(body) {
        Ok(s) => s,
        Err(e) => {
            out.push(LintViolation {
                file: name.to_string(),
                line: e.line,
                message: format!(
                    "could not tokenize file (possible injection): {}",
                    e.message
                ),
            });
            return;
        }
    };

    for st in &statements {
        let directive = st.directive.to_ascii_lowercase();
        let args = st.args.trim();
        match directive.as_str() {
            // --- always-forbidden dynamic module / scripting surface ---
            "load_module" => push(out, st.line, "load_module is forbidden".into()),
            "perl" | "perl_set" | "perl_modules" | "perl_require" => push(
                out,
                st.line,
                format!("{directive} (embedded perl) is forbidden"),
            ),
            "lua"
            | "lua_package_path"
            | "content_by_lua"
            | "content_by_lua_block"
            | "access_by_lua"
            | "access_by_lua_block"
            | "rewrite_by_lua"
            | "rewrite_by_lua_block"
            | "init_by_lua"
            | "init_by_lua_block" => push(
                out,
                st.line,
                format!("{directive} (embedded lua) is forbidden"),
            ),
            // njs: the `js_*` directive family plus the `js` import.
            "js_import"
            | "js_content"
            | "js_set"
            | "js_path"
            | "js_include"
            | "js_preload_object"
            | "js_var"
            | "js_fetch_trusted_certificate"
            | "js_periodic" => push(out, st.line, format!("{directive} (njs) is forbidden")),

            // --- filesystem escape via includes ---
            "include" => {
                if let Some(v) = check_include(args, policy) {
                    push(out, st.line, v);
                }
            }

            // --- log directives with out-of-jail paths ---
            "error_log" | "access_log" => {
                if let Some(v) = check_log(&directive, args) {
                    push(out, st.line, v);
                }
            }

            // --- document root / alias outside the public dir ---
            "root" | "alias" => {
                if let Some(v) = check_root(&directive, args, policy) {
                    push(out, st.line, v);
                }
            }

            // --- directory listing ---
            "autoindex" => {
                if args.split_whitespace().next() == Some("on") {
                    push(out, st.line, "autoindex on is forbidden".into());
                }
            }

            // --- certificates must be $acme_cert_* variables, never paths ---
            "ssl_certificate" | "ssl_certificate_key" => {
                if let Some(v) = check_ssl_cert(&directive, args) {
                    push(out, st.line, v);
                }
            }

            // --- upstream targets ---
            "proxy_pass" | "grpc_pass" | "fastcgi_pass" | "uwsgi_pass" | "scgi_pass" => {
                if let Some(v) = check_proxy_pass(&directive, args) {
                    push(out, st.line, v);
                }
            }

            _ => {}
        }
    }

    // Defence in depth: the generator always emits balanced braces. A snippet
    // that breaks out of its context (`} location /x { ... `) unbalances them.
    // The per-directive checks above already catch the *dangerous directives* a
    // breakout would use, but brace-balance catches the structural break itself
    // even if the injected directive is one we don't individually deny — and it
    // flags a snippet that closes more blocks than it opens (dropping later
    // generated directives up into a parent context).
    if let Some((line, msg)) = brace_balance_error(body) {
        out.push(LintViolation {
            file: name.to_string(),
            line: Some(line),
            message: msg,
        });
    }
}

/// Check that braces are balanced and never close below the starting depth.
/// Comments and quoted strings are ignored so a `{`/`}` in a header value or a
/// `# ...` comment does not count. Returns the offending line + message on the
/// first imbalance, or `None` when balanced.
fn brace_balance_error(body: &str) -> Option<(usize, String)> {
    let bytes = body.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut depth: i32 = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if let Some(q) = in_string {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_string = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' | b'\'' => {
                in_string = Some(c);
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth < 0 {
                    return Some((
                        line,
                        "unbalanced '}' closes a block that was never opened \
                         (possible context breakout)"
                            .to_string(),
                    ));
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    if depth != 0 {
        return Some((
            line,
            format!("unbalanced braces: {depth} block(s) left open at end of file"),
        ));
    }
    None
}

/// `include` must reference a path *under* the policy's snippets_dir (absolute).
fn check_include(args: &str, policy: &LintPolicy) -> Option<String> {
    let path = args.trim();
    if path.is_empty() {
        return Some("include with no path".into());
    }
    let p = Path::new(path);
    if !p.is_absolute() {
        return Some(format!(
            "include path {path:?} is not absolute (must be under {})",
            policy.snippets_dir.display()
        ));
    }
    if !is_under(p, &policy.snippets_dir) {
        return Some(format!(
            "include path {path:?} is outside the allowed snippets dir {}",
            policy.snippets_dir.display()
        ));
    }
    None
}

/// error_log/access_log paths must live under /var/log/angie (or be the special
/// non-path sinks Angie allows: `off`, `stderr`, `syslog:...`, `memory:...`).
fn check_log(directive: &str, args: &str) -> Option<String> {
    let target = args.split_whitespace().next().unwrap_or("");
    if target.is_empty() {
        return Some(format!("{directive} with no target"));
    }
    // access_log off; is fine. error_log has no `off`, but stderr/syslog/memory
    // are valid non-path sinks for both.
    if target == "off"
        || target == "stderr"
        || target.starts_with("syslog:")
        || target.starts_with("memory:")
    {
        return None;
    }
    let p = Path::new(target);
    if !p.is_absolute() || !is_under(p, Path::new("/var/log/angie")) {
        return Some(format!(
            "{directive} path {target:?} is outside /var/log/angie"
        ));
    }
    None
}

/// root/alias must stay under the panel's public dir (the only directory Angie
/// workers are meant to serve files from).
fn check_root(directive: &str, args: &str, policy: &LintPolicy) -> Option<String> {
    let target = args.trim();
    if target.is_empty() {
        return Some(format!("{directive} with no path"));
    }
    let p = Path::new(target);
    if !p.is_absolute() || !is_under(p, &policy.public_dir) {
        return Some(format!(
            "{directive} path {target:?} is outside the allowed public dir {}",
            policy.public_dir.display()
        ));
    }
    None
}

/// Only `$acme_cert_<name>` / `$acme_cert_key_<name>` variables are allowed —
/// never a filesystem path (which could point at an attacker-placed cert/key).
fn check_ssl_cert(directive: &str, args: &str) -> Option<String> {
    let value = args.trim();
    let ok = if directive == "ssl_certificate" {
        value.starts_with("$acme_cert_") && !value.starts_with("$acme_cert_key_")
    } else {
        value.starts_with("$acme_cert_key_")
    };
    if !ok {
        return Some(format!(
            "{directive} must reference an $acme_cert_* variable, not {value:?}"
        ));
    }
    // The remainder after the prefix must be a bare cert-name variable — no
    // extra tokens, no path characters.
    let name = value
        .trim_start_matches("$acme_cert_key_")
        .trim_start_matches("$acme_cert_");
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Some(format!(
            "{directive} references a malformed certificate variable {value:?}"
        ));
    }
    None
}

/// proxy_pass (and friends) must not target a unix socket, a loopback/link-local
/// address, or a management port (status API / panel).
fn check_proxy_pass(directive: &str, args: &str) -> Option<String> {
    let target = args.trim();
    if target.is_empty() {
        return Some(format!("{directive} with no target"));
    }
    // unix: sockets can reach arbitrary local services (including privileged
    // ones) — always denied.
    if target.contains("unix:") {
        return Some(format!(
            "{directive} to a unix socket is forbidden: {target:?}"
        ));
    }

    // Strip an optional scheme to get at host:port. A named upstream
    // (`http://host_7`) is fine — the upstream block itself was generated from a
    // validated forward_host, so we only scrutinise literal IP/host targets.
    let after_scheme = target
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(target);
    // Take the authority (up to the first '/').
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    let path = &after_scheme[authority.len()..];

    // Exempt the panel's own DNS-01 ACME hook: it is a deliberate loopback
    // proxy_pass (emitted for provider certs) to a TOKEN-GATED endpoint. This is
    // the only sanctioned loopback target. Safe because the path reaches nothing
    // else privileged and the hook returns 403 without the secret token — so
    // even a hand-written snippet aimed here cannot drive it.
    if directive == "proxy_pass" && path.trim_start_matches('/').starts_with("acme/hook") {
        return None;
    }

    if let Some((host, port)) = split_host_port(authority) {
        let is_local = match host.parse::<IpAddr>() {
            Ok(ip) => is_management_ip(ip),
            Err(_) => host.eq_ignore_ascii_case("localhost"),
        };

        // Any loopback/link-local target is denied outright: it can reach the
        // panel, the status API, or any other privileged local service, on any
        // port (PLAN.md §7: 127.0.0.0/8, ::1, link-local — no explicit override
        // in v1). The management-port callout below is a clearer message for the
        // specific 8100 / panel-port case; a non-local LAN upstream on an
        // otherwise-management port (e.g. 192.168.1.10:8080) is legitimate and
        // allowed.
        if is_local {
            if let Some(p) = port {
                if p == STATUS_PORT || p == PANEL_PORT {
                    return Some(format!(
                        "{directive} targets a local management port ({p}): {target:?}"
                    ));
                }
            }
            return Some(format!(
                "{directive} targets a loopback/link-local address: {target:?}"
            ));
        }
    }
    None
}

/// Split an authority into (host, port). Handles bracketed IPv6 (`[::1]:80`).
fn split_host_port(authority: &str) -> Option<(String, Option<u16>)> {
    if authority.is_empty() {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        // [ipv6]:port  or  [ipv6]
        let (host, after) = rest.split_once(']')?;
        let port = after.strip_prefix(':').and_then(|p| p.parse::<u16>().ok());
        return Some((host.to_string(), port));
    }
    match authority.rsplit_once(':') {
        // Only treat the tail as a port if it parses as one; otherwise the ':'
        // belonged to something else and the whole string is the host.
        Some((host, maybe_port)) => match maybe_port.parse::<u16>() {
            Ok(p) => Some((host.to_string(), Some(p))),
            Err(_) => Some((authority.to_string(), None)),
        },
        None => Some((authority.to_string(), None)),
    }
}

/// Loopback, unspecified, and link-local addresses all reach back at this host
/// (or its management surface) and are denied as proxy targets.
fn is_management_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_link_local() || v4.is_unspecified(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// True when `path` equals or is nested under `base`. Purely lexical (the files
/// don't exist yet at generation time), operating on normalized components. A
/// `..` component in `path` makes it fail closed (we reject rather than resolve).
fn is_under(path: &Path, base: &Path) -> bool {
    use std::path::Component;
    // Any parent-dir component is an escape attempt — refuse.
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let base_norm: Vec<Component> = base
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    let path_norm: Vec<Component> = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    path_norm.len() >= base_norm.len() && path_norm[..base_norm.len()] == base_norm[..]
}

// --------------------------------------------------------------- tokenizer

struct TokenizeError {
    message: String,
    line: Option<usize>,
}

/// Split config text into statements, tracking line numbers. Strips `#`
/// comments and the contents of quoted strings (so a `;` or `}` inside a string
/// literal cannot be mistaken for a statement terminator — and cannot be used
/// to hide a directive from us). Braces `{` and `}` terminate the current
/// statement just like `;` so that the *leading word* of whatever follows a
/// brace is always seen as a directive (this is what catches a snippet doing
/// `} location /x { root /;`).
fn tokenize(body: &str) -> Result<Vec<Statement>, TokenizeError> {
    let mut statements = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;
    // Start line of the token currently being accumulated.
    let mut tok_start_line = 1usize;
    let mut current = String::new();
    let mut in_string: Option<u8> = None; // Some(quote_char) while inside a string

    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\n' {
            line += 1;
        }

        if let Some(q) = in_string {
            if c == b'\\' {
                // Skip the escaped character (still counting newlines).
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
                current.push(' '); // collapse escaped content to whitespace
                continue;
            }
            if c == q {
                in_string = None;
            }
            // Replace string contents with spaces so terminators inside a
            // string are inert but token separation is preserved.
            current.push(' ');
            i += 1;
            continue;
        }

        match c {
            b'#' => {
                // Comment to end of line.
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' | b'\'' => {
                in_string = Some(c);
                current.push(' ');
                i += 1;
            }
            b';' | b'{' | b'}' => {
                flush(&mut current, tok_start_line, &mut statements);
                i += 1;
                tok_start_line = line;
            }
            _ => {
                if current.trim().is_empty() {
                    // Beginning of a fresh token run: anchor its start line.
                    tok_start_line = line;
                }
                current.push(c as char);
                i += 1;
            }
        }
    }

    if in_string.is_some() {
        return Err(TokenizeError {
            message: "unterminated quoted string".into(),
            line: Some(tok_start_line),
        });
    }
    // A trailing run of non-terminated text is fine (e.g. a final comment); we
    // simply flush whatever remains as a best-effort statement.
    flush(&mut current, tok_start_line, &mut statements);
    Ok(statements)
}

/// Turn an accumulated token run into a Statement (if non-empty) and clear it.
/// We store the leading word as `directive` and the rest as `args`; because we
/// pushed raw bytes as chars the content is ASCII for our config (multibyte
/// would only ever appear inside strings, which we blanked).
fn flush(current: &mut String, start_line: usize, out: &mut Vec<Statement>) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let directive = parts.next().unwrap_or("").to_string();
        let args = parts.next().unwrap_or("").trim().to_string();
        out.push(Statement {
            directive,
            args,
            line: start_line,
        });
    }
    current.clear();
}

#[cfg(test)]
mod tests;
