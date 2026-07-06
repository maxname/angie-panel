use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::extract::{ConnectInfo, FromRequestParts, Json, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

use crate::db::now_epoch;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "ap_session";
const SESSION_TTL: i64 = 7 * 24 * 3600;
const TOKEN_TTL: Duration = Duration::from_secs(24 * 3600);
pub const TOKEN_FILE: &str = "setup-token";
const MIN_PASSWORD_LEN: usize = 8;

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
    let token = write_setup_token(&state.cfg.data_dir)?;
    tracing::info!(
        "no admin account yet — setup token generated: {token}\n\
         open http://{}:{}/setup to create the admin \
         (token file: {})",
        state.cfg.bind_addr,
        state.cfg.port,
        state.cfg.data_dir.join(TOKEN_FILE).display()
    );
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
        .path("/")
        .max_age(time::Duration::seconds(SESSION_TTL))
        .build();
    Ok(cookie)
}

fn removal_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, ""))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build()
}

pub struct AuthUser {
    #[allow(dead_code)]
    pub user_id: i64,
    pub email: String,
}

async fn session_user(state: &AppState, jar: &CookieJar) -> Option<AuthUser> {
    let sid = jar.get(SESSION_COOKIE)?.value().to_string();
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.email FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.id = ? AND s.expires_at > ?",
    )
    .bind(&sid)
    .bind(now_epoch())
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    row.map(|(user_id, email)| AuthUser { user_id, email })
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        session_user(state, &jar)
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
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 3 {
        return Err(ApiError::bad_request("invalid_email", "invalid email"));
    }
    if req.password.len() < MIN_PASSWORD_LEN {
        return Err(ApiError::bad_request(
            "weak_password",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ));
    }
    let hash = hash_password(&state, req.password).await?;

    // v1 is single-admin: setup (including recovery) replaces the account.
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM sessions")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM users").execute(&mut *tx).await?;
    sqlx::query("INSERT INTO users (email, password_hash, created_at) VALUES (?, ?, ?)")
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
    Ok((jar.add(removal_cookie()), Json(json!({ "ok": true }))))
}

pub async fn me(user: AuthUser) -> Json<Value> {
    Json(json!({ "email": user.email }))
}
