//! Redirection hosts (301/302/… to another domain) and 404 (dead) hosts —
//! additional host types (v2). Both validate through `model`, enforce
//! domain-uniqueness ACROSS all host types, and materialize only on Apply.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, DeadHostInput, RedirectHostInput};
use crate::repo::{self, HostKind};
use crate::state::AppState;

/// Reject any domain already owned by another enabled host of any type.
async fn check_domains_unique(
    state: &AppState,
    domains: &[String],
    enabled: bool,
    exclude: (HostKind, i64),
) -> ApiResult<()> {
    if !enabled {
        return Ok(());
    }
    // `exclude.1 == 0` means "creating" (no id to skip yet).
    let skip = if exclude.1 > 0 { Some(exclude) } else { None };
    let taken = repo::all_enabled_domains(&state.db, skip).await?;
    for d in domains {
        if let Some((kind, id)) = taken.get(d) {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "domain_conflict",
                format!("domain {d} already belongs to {} #{id}", kind.label()),
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------ redirect hosts

pub async fn list_redirects(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    let hosts = repo::list_redirects(&state.db).await?;
    let arr: Vec<Value> = hosts
        .iter()
        .map(|h| serde_json::to_value(h).unwrap_or(Value::Null))
        .collect();
    Ok(Json(json!({ "redirect_hosts": arr })))
}

pub async fn get_redirect(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let h = repo::get_redirect(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no redirect host #{id}")))?;
    Ok(Json(serde_json::to_value(&h).unwrap_or(Value::Null)))
}

pub async fn create_redirect(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<RedirectHostInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_redirect_input(raw, state.cfg.allow_advanced_snippets)?;
    check_domains_unique(
        &state,
        &input.domains,
        input.enabled,
        (HostKind::Redirect, 0),
    )
    .await?;
    let id = repo::insert_redirect(&state.db, &input).await?;
    let h = repo::get_redirect(&state.db, id)
        .await?
        .expect("just inserted");
    Ok(Json(serde_json::to_value(&h).unwrap_or(Value::Null)))
}

pub async fn update_redirect(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<RedirectHostInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_redirect_input(raw, state.cfg.allow_advanced_snippets)?;
    check_domains_unique(
        &state,
        &input.domains,
        input.enabled,
        (HostKind::Redirect, id),
    )
    .await?;
    if !repo::update_redirect(&state.db, id, &input).await? {
        return Err(ApiError::not_found(format!("no redirect host #{id}")));
    }
    let h = repo::get_redirect(&state.db, id)
        .await?
        .expect("just updated");
    Ok(Json(serde_json::to_value(&h).unwrap_or(Value::Null)))
}

pub async fn delete_redirect(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if !repo::delete_redirect(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no redirect host #{id}")));
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn enable_redirect(
    u: AuthUser,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> ApiResult<Json<Value>> {
    set_redirect_enabled(u, state, id, true).await
}
pub async fn disable_redirect(
    u: AuthUser,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> ApiResult<Json<Value>> {
    set_redirect_enabled(u, state, id, false).await
}
async fn set_redirect_enabled(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    enabled: bool,
) -> ApiResult<Json<Value>> {
    if enabled {
        if let Some(h) = repo::get_redirect(&state.db, id).await? {
            check_domains_unique(&state, &h.domains, true, (HostKind::Redirect, id)).await?;
        }
    }
    if !repo::set_redirect_enabled(&state.db, id, enabled).await? {
        return Err(ApiError::not_found(format!("no redirect host #{id}")));
    }
    Ok(Json(json!({ "ok": true, "enabled": enabled })))
}

// ---------------------------------------------------------------- dead hosts

pub async fn list_dead(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let hosts = repo::list_dead(&state.db).await?;
    let arr: Vec<Value> = hosts
        .iter()
        .map(|h| serde_json::to_value(h).unwrap_or(Value::Null))
        .collect();
    Ok(Json(json!({ "dead_hosts": arr })))
}

pub async fn get_dead(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let h = repo::get_dead(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no 404 host #{id}")))?;
    Ok(Json(serde_json::to_value(&h).unwrap_or(Value::Null)))
}

pub async fn create_dead(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<DeadHostInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_dead_input(raw, state.cfg.allow_advanced_snippets)?;
    check_domains_unique(&state, &input.domains, input.enabled, (HostKind::Dead, 0)).await?;
    let id = repo::insert_dead(&state.db, &input).await?;
    let h = repo::get_dead(&state.db, id).await?.expect("just inserted");
    Ok(Json(serde_json::to_value(&h).unwrap_or(Value::Null)))
}

pub async fn update_dead(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<DeadHostInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_dead_input(raw, state.cfg.allow_advanced_snippets)?;
    check_domains_unique(&state, &input.domains, input.enabled, (HostKind::Dead, id)).await?;
    if !repo::update_dead(&state.db, id, &input).await? {
        return Err(ApiError::not_found(format!("no 404 host #{id}")));
    }
    let h = repo::get_dead(&state.db, id).await?.expect("just updated");
    Ok(Json(serde_json::to_value(&h).unwrap_or(Value::Null)))
}

pub async fn delete_dead(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if !repo::delete_dead(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no 404 host #{id}")));
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn enable_dead(
    u: AuthUser,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> ApiResult<Json<Value>> {
    set_dead_enabled(u, state, id, true).await
}
pub async fn disable_dead(
    u: AuthUser,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> ApiResult<Json<Value>> {
    set_dead_enabled(u, state, id, false).await
}
async fn set_dead_enabled(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    enabled: bool,
) -> ApiResult<Json<Value>> {
    if enabled {
        if let Some(h) = repo::get_dead(&state.db, id).await? {
            check_domains_unique(&state, &h.domains, true, (HostKind::Dead, id)).await?;
        }
    }
    if !repo::set_dead_enabled(&state.db, id, enabled).await? {
        return Err(ApiError::not_found(format!("no 404 host #{id}")));
    }
    Ok(Json(json!({ "ok": true, "enabled": enabled })))
}
