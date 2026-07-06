use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use crate::{assets, auth, security, system};

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/auth/state", get(auth::auth_state))
        .route("/auth/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/system/status", get(system::get_status))
        .route(
            "/system/configtest",
            get(system::last_configtest).post(system::run_configtest),
        )
        .fallback(security::api_not_found);

    Router::new()
        .nest("/api", api)
        .fallback(get(assets::static_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::security_layer,
        ))
        .with_state(state)
}
