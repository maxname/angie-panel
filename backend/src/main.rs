mod access_lists;
mod acme_hook;
mod api;
mod apply;
mod apply_api;
mod assets;
mod audit;
mod auth;
mod bans;
mod certs;
mod config;
mod dashboard;
mod db;
mod dns_providers;
mod error;
mod export_import;
mod generator;
mod geo;
mod health;
mod helper;
mod hosts;
mod model;
mod other_hosts;
mod reconcile;
mod repo;
mod secretbox;
mod security;
mod settings;
mod sni_routers;
mod state;
mod streams;
mod system;
mod systemd;
mod tokens;
mod users;

#[cfg(test)]
mod integration_tests;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};

use crate::state::AppState;

#[derive(Parser)]
#[command(
    name = "angie-panel",
    version,
    about = "Web configurator for the Angie reverse proxy"
)]
struct Cli {
    /// Config file (default: $ANGIE_PANEL_CONFIG, /etc/angie-panel.toml, ./angie-panel.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the panel web server (default)
    Serve,
    /// Privileged helper — invoked by the root oneshot systemd units only
    Helper {
        #[command(subcommand)]
        mode: HelperMode,
    },
    /// Generate a new one-time setup token (admin password recovery)
    ResetPassword,
}

#[derive(Subcommand)]
enum HelperMode {
    /// Validate the live Angie configuration (angie -t)
    Configtest,
    /// Apply staged configuration (M1, not implemented yet)
    Apply,
    /// Activate the Angie stream {} context in the live angie.conf (one-time)
    EnableStreams,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg_path = config::resolve_path(cli.config)?;
    let cfg = config::load(&cfg_path)?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(cfg, cfg_path).await,
        Command::Helper { mode } => match mode {
            HelperMode::Configtest => helper::configtest(&cfg).await,
            HelperMode::Apply => helper::apply(&cfg).await,
            HelperMode::EnableStreams => helper::enable_streams(&cfg).await,
        },
        Command::ResetPassword => reset_password(cfg),
    }
}

fn reset_password(cfg: config::PanelConfig) -> anyhow::Result<()> {
    let token = auth::write_setup_token(&cfg.data_dir)?;
    println!(
        "One-time setup token (valid 24h):\n\n    {token}\n\n\
         Open http://{}:{}/setup and use it to (re)create the admin account.\n\
         Existing hosts/certificates data is NOT affected.",
        cfg.bind_addr, cfg.port
    );
    Ok(())
}

async fn serve(cfg: config::PanelConfig, cfg_path: PathBuf) -> anyhow::Result<()> {
    std::fs::create_dir_all(&cfg.data_dir)
        .with_context(|| format!("creating data dir {}", cfg.data_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&cfg.data_dir, std::fs::Permissions::from_mode(0o700));
    }

    let pool = db::connect(&cfg.data_dir).await?;
    let bind = format!("{}:{}", cfg.bind_addr, cfg.port);
    let state = Arc::new(AppState::new(cfg, cfg_path, pool));

    auth::bootstrap_setup_token(&state).await?;
    auth::bootstrap_cli_token(&state).await?;

    // Seal any DNS credential written before encryption-at-rest landed, so no
    // plaintext token survives an upgrade.
    match acme_hook::seal_legacy_credentials(&state).await {
        Ok(0) => {}
        Ok(n) => tracing::info!("sealed {n} DNS credential value(s) stored in the clear"),
        Err(e) => tracing::error!(error = %e, "could not seal legacy DNS credentials"),
    }

    // Crash recovery (PLAN.md §2.2): if a prior apply was interrupted, restore
    // the live config from the last snapshot when it no longer validates.
    match apply::recover(&state.cfg).await {
        Ok(outcome) => tracing::debug!(?outcome, "apply crash-recovery check"),
        Err(e) => tracing::warn!(error = %e, "apply crash-recovery check failed"),
    }

    // Background reconciler: auto-activate HTTPS once certificates are issued.
    reconcile::spawn(state.clone());
    health::spawn(state.clone());

    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::info!("angie-panel listening on http://{bind}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
