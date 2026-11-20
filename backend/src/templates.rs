//! Templates: a saved set of settings to start a new proxy host or stream
//! from, instead of filling the editor in again for every host that is
//! configured like the last one.
//!
//! A template is a preset, not a link. Creating from one produces an ordinary
//! host or stream through the ordinary create endpoint; the template is not
//! consulted again, and editing it later changes nothing that already exists.
//! So nothing here reaches the generated config — but a template's settings
//! are still held to the rules of the thing it creates (see
//! `model::validate_template_input`), rather than stored as an opaque blob to
//! be judged only once a host is made of them.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, Template, TemplateInput, UpstreamPolicy};
use crate::repo;
use crate::state::AppState;

fn template_json(t: &Template) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

fn not_found(id: i64) -> ApiError {
    ApiError::not_found(format!("no template #{id}"))
}

/// Validate a template and check that everything it points at exists. A
/// reference to something that is not there would only surface later, as a
/// refusal to create the host — at the worst moment to find out.
async fn prepare(
    state: &AppState,
    raw: TemplateInput,
    except: Option<i64>,
) -> ApiResult<TemplateInput> {
    let policy = UpstreamPolicy {
        allow_loopback: state.cfg.allow_loopback_upstreams,
    };
    let input = model::validate_template_input(raw, state.cfg.allow_advanced_snippets, &policy)?;

    let refs = model::template_refs(&input.config);
    if let Some(id) = refs.certificate_id {
        if repo::get_cert(&state.db, id).await?.is_none() {
            return Err(ApiError::not_found(format!("no certificate #{id}")));
        }
    }
    if let Some(id) = refs.access_list_id {
        if repo::get_access_list(&state.db, id).await?.is_none() {
            return Err(ApiError::not_found(format!("no access list #{id}")));
        }
    }
    if let Some(id) = refs.upstream_pool_id {
        if repo::get_upstream_pool(&state.db, id).await?.is_none() {
            return Err(ApiError::not_found(format!("no upstream pool #{id}")));
        }
    }
    if let Some(profile) = &refs.dns_provider {
        let exists = match profile.parse::<i64>() {
            Ok(id) => repo::get_dns_credential(&state.db, id).await?.is_some(),
            Err(_) => false,
        };
        if !exists {
            return Err(ApiError::bad_request(
                "invalid_dns_provider",
                "the selected DNS provider profile does not exist",
            ));
        }
    }

    // Templates are picked by name from a per-kind list.
    if repo::template_name_taken(&state.db, input.kind, &input.name, except).await? {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "template_name_taken",
            format!("a template named '{}' already exists", input.name),
        ));
    }
    Ok(input)
}

pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let templates = repo::list_templates(&state.db).await?;
    let arr: Vec<Value> = templates.iter().map(template_json).collect();
    Ok(Json(json!({ "templates": arr })))
}

pub async fn get_one(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let template = repo::get_template(&state.db, id)
        .await?
        .ok_or_else(|| not_found(id))?;
    Ok(Json(template_json(&template)))
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<TemplateInput>,
) -> ApiResult<Json<Value>> {
    let input = prepare(&state, raw, None).await?;
    let id = repo::insert_template(&state.db, &input).await?;
    let template = repo::get_template(&state.db, id)
        .await?
        .expect("just inserted");
    Ok(Json(template_json(&template)))
}

pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<TemplateInput>,
) -> ApiResult<Json<Value>> {
    let existing = repo::get_template(&state.db, id)
        .await?
        .ok_or_else(|| not_found(id))?;
    // A host template cannot become a stream template: its settings would
    // mean nothing. Make a new one instead.
    if raw.kind != existing.kind {
        return Err(ApiError::bad_request(
            "invalid_template",
            "a template's kind cannot be changed",
        ));
    }
    let input = prepare(&state, raw, Some(id)).await?;
    if !repo::update_template(&state.db, id, &input).await? {
        return Err(not_found(id));
    }
    let template = repo::get_template(&state.db, id)
        .await?
        .expect("just updated");
    Ok(Json(template_json(&template)))
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    // Nothing points at a template (hosts made from one keep no link), so
    // there is no in-use check to make.
    if !repo::delete_template(&state.db, id).await? {
        return Err(not_found(id));
    }
    Ok(Json(json!({ "ok": true })))
}
