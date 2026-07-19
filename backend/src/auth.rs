use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::extract::{ConnectInfo, FromRequestParts, Json, State};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

use crate::db::now_epoch;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "ap_session";
const SESSION_TTL: i64 = 7 * 24 * 3600;
const TOKEN_TTL: Duration = Duration::from_secs(24 * 3600);
pub const TOKEN_FILE: &str = "setup-token";
pub const MIN_PASSWORD_LEN: usize = 8;

/// Prefix on every API token. Makes the secret recognizable in a shell history
/// or a CI log, and gives secret scanners something to match.
pub const API_TOKEN_PREFIX: &str = "ap_";
/// File in the data dir holding the machine-local token used by `apctl`.
pub const CLI_TOKEN_FILE: &str = "cli-token";
/// `last_used_at` is refreshed at most this often, so a busy API does not turn
/// every read into a write.
const TOKEN_USE_STAMP_INTERVAL: i64 = 60;

/// Operator role. `admin` may change anything; `viewer` is read-only (enforced
/// centrally in `security::security_layer` — every mutating request from a
/// non-admin is rejected there, so no handler can forget the check).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Viewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Viewer => "viewer",
        }
    }
    pub fn from_str(s: &str) -> Role {
        // Fail safe: anything unrecognized is the LEAST-privileged role.
        match s {
            "admin" => Role::Admin,
            _ => Role::Viewer,
        }
    }
}

/// Validate + normalize an email (shared by setup and user creation).
pub fn normalize_email(raw: &str) -> Result<String, ApiError> {
    let email = raw.trim().to_lowercase();
    if !email.contains('@')
        || email.len() < 3
        || email.len() > 254
        || email.contains(char::is_whitespace)
    {
        return Err(ApiError::bad_request("invalid_email", "invalid email"));
    }
    Ok(email)
}

// ---------------------------------------------------------------- passwords

fn argon2() -> Argon2<'static> {
    // OWASP-recommended interactive parameters: m=19 MiB, t=2, p=1.
    let params = Params::new(19 * 1024, 2, 1, None).expect("argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub async fn hash_password(state: &AppState, password: String) -> ApiResult<String> {
    let _permit = state
        .argon_sem
        .acquire()
        .await
        .map_err(ApiError::internal)?;
    let mut salt_bytes = [0u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(ApiError::internal)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(ApiError::internal)?;
    tokio::task::spawn_blocking(move || {
        argon2()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)
}

pub async fn verify_password(state: &AppState, password: String, hash: String) -> ApiResult<bool> {
    let _permit = state
        .argon_sem
        .acquire()
        .await
        .map_err(ApiError::internal)?;
    tokio::task::spawn_blocking(move || {
        let Ok(parsed) = PasswordHash::new(&hash) else {
            return false;
        };
        argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
    .await
    .map_err(ApiError::internal)
}

/// A syntactically valid hash of a random value, used to equalize timing
/// when the user does not exist.
fn dummy_hash() -> String {
    "$argon2id$v=19$m=19456,t=2,p=1$YW5naWVwYW5lbGR1bW15$\
     kW68mVVDDl1eBMc4y3zpCLQyv2sf6wRcyyFgqcnfMFo"
        .to_string()
}

// ------------------------------------------------------------- setup token

/// Generate a fresh one-time setup token and write it (0600) into data_dir.
pub fn write_setup_token(data_dir: &Path) -> anyhow::Result<String> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(|e| anyhow::anyhow!("getrandom: {e}"))?;
    let token = hex::encode(buf);
    let path = data_dir.join(TOKEN_FILE);
    std::fs::write(&path, format!("{token}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(token)
}

enum TokenState {
    Missing,
    Expired,
    Valid(String),
}

fn read_setup_token(data_dir: &Path) -> TokenState {
    let path = data_dir.join(TOKEN_FILE);
    let Ok(meta) = std::fs::metadata(&path) else {
        return TokenState::Missing;
    };
    let fresh = meta
        .modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .map(|age| age < TOKEN_TTL)
        .unwrap_or(false);
    if !fresh {
        return TokenState::Expired;
    }
    match std::fs::read_to_string(&path) {
        Ok(s) => TokenState::Valid(s.trim().to_string()),
        Err(_) => TokenState::Missing,
    }
}

/// Called on startup: make sure a setup path exists when there is no admin.
pub async fn bootstrap_setup_token(state: &AppState) -> anyhow::Result<()> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;
    if users > 0 {
        return Ok(());
    }
    if let TokenState::Valid(_) = read_setup_token(&state.cfg.data_dir) {
        tracing::info!("setup token already present, reusing it");
        return Ok(());
    }
    let _token = write_setup_token(&state.cfg.data_dir)?;
    // Do NOT log the token itself — journald is typically readable by more
    // principals than the 0600 token file, and this token is the sole gate on
    // the break-glass /setup path (which wipes all users). Point at the file.
    tracing::info!(
        "no admin account yet — setup token written to {} (mode 0600); \
         open http://{}:{}/setup to create the admin",
        state.cfg.data_dir.join(TOKEN_FILE).display(),
        state.cfg.bind_addr,
        state.cfg.port,
    );
    Ok(())
}

/// Ensure the machine-local `apctl` token exists and matches the DB.
///
/// This is what makes the CLI zero-config on the box: no login, no token to
/// paste. It grants no new authority — the file is 0600 in a 0700 data dir, so
/// its readers (the service account and root) can already read the database
/// itself. Delete the file and restart to rotate it.
pub async fn bootstrap_cli_token(state: &AppState) -> anyhow::Result<()> {
    let path = state.cfg.data_dir.join(CLI_TOKEN_FILE);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let hash = hash_api_token(existing.trim());
        let known: Option<i64> =
            sqlx::query_scalar("SELECT id FROM api_tokens WHERE token_hash = ? AND is_local = 1")
                .bind(&hash)
                .fetch_optional(&state.db)
                .await?;
        if known.is_some() {
            return Ok(());
        }
        tracing::warn!(
            "{} does not match any local token in the database — reissuing",
            path.display()
        );
    }

    let (secret, hash, prefix) = new_api_token()
        .map_err(|e| anyhow::anyhow!("generating local CLI token: {}", e.message))?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM api_tokens WHERE is_local = 1")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO api_tokens (name, token_hash, prefix, user_id, is_local, created_at) \
         VALUES ('apctl', ?, ?, NULL, 1, ?)",
    )
    .bind(&hash)
    .bind(&prefix)
    .bind(now_epoch())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    std::fs::write(&path, format!("{secret}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    tracing::info!("local CLI token written to {} (mode 0600)", path.display());
    Ok(())
}

// ---------------------------------------------------------------- sessions

fn new_session_id() -> ApiResult<String> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(ApiError::internal)?;
    Ok(hex::encode(buf))
}

async fn create_session(state: &AppState, user_id: i64) -> ApiResult<Cookie<'static>> {
    let id = new_session_id()?;
    let now = now_epoch();
    sqlx::query("INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(user_id)
        .bind(now)
        .bind(now + SESSION_TTL)
        .execute(&state.db)
        .await?;
    // Opportunistically drop expired sessions.
    let _ = sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
        .bind(now)
        .execute(&state.db)
        .await;
    let cookie = Cookie::build((SESSION_COOKIE, id))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.cfg.secure_cookies)
        .path("/")
        .max_age(time::Duration::seconds(SESSION_TTL))
        .build();
    Ok(cookie)
}

fn removal_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, ""))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build()
}

// ------------------------------------------------------------- API tokens

/// Generate a token secret and its storage form: `(secret, hash, prefix)`.
/// Only the hash and prefix are persisted — the secret is shown once and is
/// unrecoverable afterwards.
pub fn new_api_token() -> ApiResult<(String, String, String)> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(ApiError::internal)?;
    let body = hex::encode(buf);
    let secret = format!("{API_TOKEN_PREFIX}{body}");
    let hash = hash_api_token(&secret);
    let prefix = body[..8].to_string();
    Ok((secret, hash, prefix))
}

/// Storage hash for a token secret.
///
/// Plain SHA-256, deliberately not argon2: the secret is 256 CSPRNG bits, so
/// there is no dictionary to slow down, and a password KDF would run on every
/// authenticated API request. Argon2 remains in use for human passwords, where
/// the work factor is what actually buys security.
pub fn hash_api_token(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(secret.trim().as_bytes()))
}

/// Token names end up in the audit log, so keep them to plain printable text.
pub fn normalize_token_name(raw: &str) -> Result<String, ApiError> {
    let name = raw.trim();
    let ok = !name.is_empty()
        && name.chars().count() <= 40
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'));
    if !ok {
        return Err(ApiError::bad_request(
            "invalid_name",
            "token name must be 1-40 chars of letters, digits, space, dot, underscore or dash",
        ));
    }
    Ok(name.to_string())
}

/// Who is making this request. A session and a token differ in more than
/// bookkeeping: the machine-local token has no owning account at all, so
/// endpoints that act on "my user" must not silently pick one.
pub struct AuthUser {
    /// The owning account. `None` for the machine-local `apctl` token.
    pub user_id: Option<i64>,
    /// Actor label for the audit log — an email for sessions, an email plus
    /// token name for API tokens.
    pub email: String,
    pub role: Role,
    /// `Some(name)` when authenticated by an API token rather than a cookie.
    pub token_name: Option<String>,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
    /// 403 unless this user is an admin (handler-side guard for read endpoints
    /// the method-based middleware doesn't cover, e.g. GET /api/users).
    pub fn require_admin(&self) -> ApiResult<()> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(ApiError::forbidden(
                "forbidden",
                "this action requires an administrator",
            ))
        }
    }

    /// The owning account id, for endpoints that act on "the current user".
    /// Errors for the machine-local token, which has no account — better a
    /// clean 403 than operating on an arbitrary one.
    pub fn require_user_id(&self) -> ApiResult<i64> {
        self.user_id.ok_or_else(|| {
            ApiError::forbidden(
                "no_account",
                "this token is not tied to a user account; use a browser session",
            )
        })
    }

    /// Reject API tokens. For flows that only make sense in a browser —
    /// changing your own password, ending your own session.
    pub fn require_session(&self) -> ApiResult<()> {
        if self.token_name.is_some() {
            return Err(ApiError::forbidden(
                "session_required",
                "this action requires a browser session, not an API token",
            ));
        }
        Ok(())
    }
}

async fn session_user(state: &AppState, jar: &CookieJar) -> Option<AuthUser> {
    let sid = jar.get(SESSION_COOKIE)?.value().to_string();
    let row: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.role FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.id = ? AND s.expires_at > ?",
    )
    .bind(&sid)
    .bind(now_epoch())
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    row.map(|(user_id, email, role)| AuthUser {
        user_id: Some(user_id),
        email,
        role: Role::from_str(&role),
        token_name: None,
    })
}

/// Resolve an `Authorization: Bearer <secret>` header against `api_tokens`.
async fn token_user(state: &AppState, headers: &HeaderMap) -> Option<AuthUser> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())?;
    let secret = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?
        .trim();
    if !secret.starts_with(API_TOKEN_PREFIX) {
        return None;
    }
    let hash = hash_api_token(secret);
    let now = now_epoch();
    // Looked up BY the hash: an attacker cannot probe this with timing without
    // already knowing the 256-bit secret, so no constant-time compare is needed.
    // The role comes from the owning account, so demoting a user immediately
    // demotes their tokens; the local token has no account and is admin.
    type TokenRow = (
        i64,
        String,
        i64,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<i64>,
    );
    let row: Option<TokenRow> = sqlx::query_as(
        "SELECT t.id, t.name, t.is_local, t.user_id, u.email, u.role, t.last_used_at \
             FROM api_tokens t LEFT JOIN users u ON u.id = t.user_id \
             WHERE t.token_hash = ? AND (t.expires_at IS NULL OR t.expires_at > ?)",
    )
    .bind(&hash)
    .bind(now)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let (token_id, name, is_local, user_id, owner_email, owner_role, last_used) = row?;

    // A token whose owner row is gone is dead. The FK cascades on user delete,
    // so this only guards against a hand-edited database.
    if is_local == 0 && owner_email.is_none() {
        return None;
    }

    if last_used.is_none_or(|t| now - t >= TOKEN_USE_STAMP_INTERVAL) {
        let _ = sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(token_id)
            .execute(&state.db)
            .await;
    }

    let (email, role) = match owner_email {
        Some(e) => (
            format!("{e} (token: {name})"),
            Role::from_str(owner_role.as_deref().unwrap_or("")),
        ),
        // Machine-local token: root on this host, hence admin.
        None => (format!("apctl (local token: {name})"), Role::Admin),
    };
    Some(AuthUser {
        user_id,
        email,
        role,
        token_name: Some(name),
    })
}

/// The single principal resolver. Everything that needs to know who is calling
/// goes through here — the `AuthUser` extractor, the role gate and the audit
/// log alike. Keeping it in one place is what stops a token from satisfying a
/// handler while being invisible to the middleware that guards it.
pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> Option<AuthUser> {
    if let Some(u) = token_user(state, headers).await {
        return Some(u);
    }
    let jar = CookieJar::from_headers(headers);
    session_user(state, &jar).await
}

/// The caller's role, from raw request headers. Used by the security middleware
/// to gate mutations. `None` = no/invalid credentials (the handler's `AuthUser`
/// extractor then returns a clean 401).
pub async fn session_role(state: &AppState, headers: &HeaderMap) -> Option<Role> {
    authenticate(state, headers).await.map(|u| u.role)
}

/// The caller's audit label from raw headers. `None` = unauthenticated — e.g. a
/// login request.
pub async fn session_email(state: &AppState, headers: &HeaderMap) -> Option<String> {
    authenticate(state, headers).await.map(|u| u.email)
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        authenticate(state, &parts.headers)
            .await
            .ok_or_else(ApiError::unauthorized)
    }
}

// ---------------------------------------------------------------- handlers

#[derive(Deserialize)]
pub struct SetupRequest {
    token: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

pub async fn auth_state(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> ApiResult<Json<Value>> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;
    let token_valid = matches!(read_setup_token(&state.cfg.data_dir), TokenState::Valid(_));
    let setup_required = users == 0 || token_valid;
    let authenticated = session_user(&state, &jar).await.is_some();
    Ok(Json(
        json!({ "setup_required": setup_required, "authenticated": authenticated }),
    ))
}

pub async fn setup(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(req): Json<SetupRequest>,
) -> ApiResult<(CookieJar, Json<Value>)> {
    if !state.setup_limiter.check(addr.ip()) {
        return Err(ApiError::too_many_requests());
    }
    let expected = match read_setup_token(&state.cfg.data_dir) {
        TokenState::Valid(t) => t,
        TokenState::Expired => {
            return Err(ApiError::forbidden(
                "token_expired",
                "setup token expired; run `angie-panel reset-password` on the server",
            ));
        }
        TokenState::Missing => {
            return Err(ApiError::forbidden(
                "setup_disabled",
                "setup is not active; run `angie-panel reset-password` on the server",
            ));
        }
    };
    let supplied = req.token.trim();
    if expected.as_bytes().ct_eq(supplied.as_bytes()).unwrap_u8() != 1 {
        return Err(ApiError::forbidden("invalid_token", "invalid setup token"));
    }
    let email = normalize_email(&req.email)?;
    if req.password.len() < MIN_PASSWORD_LEN {
        return Err(ApiError::bad_request(
            "weak_password",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ));
    }
    let hash = hash_password(&state, req.password).await?;

    // Setup is the break-glass recovery path: it resets to a SINGLE admin
    // (wiping any other accounts and sessions). Normal multi-user management is
    // done by that admin via /api/users.
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM sessions")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM users").execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO users (email, password_hash, role, created_at) VALUES (?, ?, 'admin', ?)",
    )
    .bind(&email)
    .bind(&hash)
    .bind(now_epoch())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let _ = std::fs::remove_file(state.cfg.data_dir.join(TOKEN_FILE));
    tracing::info!(email = %email, "admin account created via setup token");

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_one(&state.db)
        .await?;
    let cookie = create_session(&state, user_id).await?;
    Ok((jar.add(cookie), Json(json!({ "ok": true }))))
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> ApiResult<(CookieJar, Json<Value>)> {
    if !state.login_limiter.check(addr.ip()) {
        return Err(ApiError::too_many_requests());
    }
    let email = req.email.trim().to_lowercase();
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE email = ?")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    let (user_id, hash, exists) = match row {
        Some((id, h)) => (id, h, true),
        None => (0, dummy_hash(), false),
    };
    let ok = verify_password(&state, req.password, hash).await?;
    if !(ok && exists) {
        tracing::warn!(email = %email, ip = %addr.ip(), "failed login attempt");
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "invalid email or password",
        ));
    }
    let cookie = create_session(&state, user_id).await?;
    tracing::info!(email = %email, "login");
    Ok((jar.add(cookie), Json(json!({ "ok": true }))))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<Value>)> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        let _ = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(c.value())
            .execute(&state.db)
            .await;
    }
    Ok((
        jar.add(removal_cookie(state.cfg.secure_cookies)),
        Json(json!({ "ok": true })),
    ))
}

pub async fn me(user: AuthUser) -> Json<Value> {
    Json(json!({ "email": user.email, "role": user.role }))
}
