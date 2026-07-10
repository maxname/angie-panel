use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use crate::{apply_api, assets, auth, certs, hosts, security, system};

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
        .route("/hosts", get(hosts::list).post(hosts::create))
        .route(
            "/hosts/{id}",
            get(hosts::get_one).put(hosts::update).delete(hosts::delete),
        )
        .route("/hosts/{id}/enable", post(hosts::enable))
        .route("/hosts/{id}/disable", post(hosts::disable))
        .route("/certificates", get(certs::list).post(certs::create))
        .route(
            "/certificates/{id}",
            get(certs::get_one).delete(certs::delete),
        )
        .route("/certificates/{id}/precheck", post(certs::precheck))
        .route("/apply/preview", get(apply_api::preview))
        .route("/apply", post(apply_api::apply_now))
        .route("/apply/history", get(apply_api::history))
        .route(
            "/settings",
            get(apply_api::get_settings).put(apply_api::put_settings),
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
