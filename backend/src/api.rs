use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use crate::{
    access_lists, apply_api, assets, auth, certs, dashboard, export_import, hosts, other_hosts,
    security, system,
};

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
        .route(
            "/access-lists",
            get(access_lists::list).post(access_lists::create),
        )
        .route(
            "/access-lists/{id}",
            get(access_lists::get_one)
                .put(access_lists::update)
                .delete(access_lists::delete),
        )
        .route(
            "/redirect-hosts",
            get(other_hosts::list_redirects).post(other_hosts::create_redirect),
        )
        .route(
            "/redirect-hosts/{id}",
            get(other_hosts::get_redirect)
                .put(other_hosts::update_redirect)
                .delete(other_hosts::delete_redirect),
        )
        .route(
            "/redirect-hosts/{id}/enable",
            post(other_hosts::enable_redirect),
        )
        .route(
            "/redirect-hosts/{id}/disable",
            post(other_hosts::disable_redirect),
        )
        .route(
            "/dead-hosts",
            get(other_hosts::list_dead).post(other_hosts::create_dead),
        )
        .route(
            "/dead-hosts/{id}",
            get(other_hosts::get_dead)
                .put(other_hosts::update_dead)
                .delete(other_hosts::delete_dead),
        )
        .route("/dead-hosts/{id}/enable", post(other_hosts::enable_dead))
        .route("/dead-hosts/{id}/disable", post(other_hosts::disable_dead))
        .route("/apply/preview", get(apply_api::preview))
        .route("/apply", post(apply_api::apply_now))
        .route("/apply/history", get(apply_api::history))
        .route(
            "/settings",
            get(apply_api::get_settings).put(apply_api::put_settings),
        )
        .route("/dashboard", get(dashboard::get_dashboard))
        .route("/export", get(export_import::export))
        .route("/import", post(export_import::import))
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
