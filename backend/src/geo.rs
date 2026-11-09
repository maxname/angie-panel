//! Global country access policy (GeoIP blocking). A single panel-wide policy —
//! off, deny-listed, or allow-listed countries — resolved to CIDR ranges from
//! the bundled dataset and enforced as a `geo` map plus a per-host
//! `if (…) return 403` (see the generator). Takes effect on the next Apply.
//!
//! Mutations are admin-only (enforced centrally in `security::security_layer`).

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::Value;

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::model::{self, GeoPolicy};
use crate::settings;
use crate::state::AppState;

pub async fn get(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let policy = settings::geo_policy(&state).await?;
    Ok(Json(serde_json::to_value(&policy).unwrap_or(Value::Null)))
}

pub async fn put(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<GeoPolicy>,
) -> ApiResult<Json<Value>> {
    let policy = model::validate_geo_policy(raw)?;
    settings::set_geo_policy(&state, &policy).await?;
    Ok(Json(serde_json::to_value(&policy).unwrap_or(Value::Null)))
}
