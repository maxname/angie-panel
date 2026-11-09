//! Audit log viewer. Entries are written centrally by `security::security_layer`
//! for every mutating request that reaches a handler; this endpoint just lists
//! the most recent. Admin-only — GET is not covered by the mutation role gate,
//! so the handler checks the role itself (like `users::list`).

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::repo;
use crate::state::AppState;

const AUDIT_LIST_LIMIT: i64 = 200;

pub async fn list(u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    u.require_admin()?;
    let entries = repo::list_audit(&state.db, AUDIT_LIST_LIMIT).await?;
    let arr: Vec<Value> = entries
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
        .collect();
    Ok(Json(json!({ "entries": arr })))
}
