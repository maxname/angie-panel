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
mod ctl;
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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};

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
    /// Operator CLI over the panel's API (also installed as `apctl`)
    Ctl {
        #[command(flatten)]
        opts: CtlOptions,
        #[command(subcommand)]
        command: ctl::CtlCommand,
    },
    /// Write angie-panel.1 and apctl.1 into DIR. Packaging-time only: the
    /// release build runs this on the host, since the cross-compiled musl
    /// binary cannot be executed on the CI runner.
    #[command(hide = true)]
    Man { dir: PathBuf },
}

/// Render both spellings' man pages. Generated rather than committed so they
/// cannot drift from the clap definitions they document.
fn write_man_pages(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for (name, cmd) in [
        ("angie-panel.1", Cli::command()),
        ("apctl.1", CtlCli::command()),
    ] {
        let path = dir.join(name);
        let mut buf = Vec::new();
        clap_mangen::Man::new(cmd)
            .render(&mut buf)
            .with_context(|| format!("rendering {name}"))?;
        std::fs::write(&path, buf).with_context(|| format!("writing {}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(())
}

/// Shared by `angie-panel ctl` and the `apctl` entry point, so the two spellings
/// can never drift apart.
#[derive(clap::Args)]
struct CtlOptions {
    /// Panel URL (default: $ANGIE_PANEL_URL, else the configured bind address)
    #[arg(long, global = true)]
    url: Option<String>,
    /// API token (default: $ANGIE_PANEL_TOKEN, else the local token file)
    #[arg(long, global = true)]
    token: Option<String>,
    /// Print the raw API response instead of a human-readable summary
    #[arg(long, global = true)]
    json: bool,
}

/// The `apctl` spelling: the same subcommands, hoisted to the top level.
#[derive(Parser)]
#[command(
    name = "apctl",
    version,
    about = "Operator CLI for the Angie panel (same as `angie-panel ctl`)"
)]
struct CtlCli {
    /// Config file (default: $ANGIE_PANEL_CONFIG, /etc/angie-panel.toml, ./angie-panel.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(flatten)]
    opts: CtlOptions,
    #[command(subcommand)]
    command: ctl::CtlCommand,
}

/// True when argv[0] is the `apctl` symlink rather than the panel binary.
fn invoked_as_ctl() -> bool {
    std::env::args_os()
        .next()
        .map(PathBuf::from)
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .is_some_and(|name| name == "apctl")
}

/// Config for the CLI. Unlike the server, a missing config file is not fatal:
/// with `--url` and `--token` there is nothing to read from it.
fn ctl_config(explicit: Option<PathBuf>) -> config::PanelConfig {
    config::resolve_path(explicit)
        .and_then(|p| config::load(&p))
        .unwrap_or_else(|_| toml::from_str("").expect("PanelConfig defaults"))
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
    // The CLI talks to an operator, not to journald: keep its stderr for real
    // problems instead of the server's info-level narration.
    let default_filter = if invoked_as_ctl() {
        "warn"
    } else {
        "info,sqlx=warn"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .init();

    if invoked_as_ctl() {
        let cli = CtlCli::parse();
        // Before anything that needs a token or a reachable panel.
        if let ctl::CtlCommand::Completions { shell } = cli.command {
            ctl::print_completions(shell, &mut CtlCli::command());
            return Ok(());
        }
        let cfg = ctl_config(cli.config);
        return ctl::run(
            cli.command,
            cfg,
            cli.opts.url,
            cli.opts.token,
            cli.opts.json,
        )
        .await;
    }

    let cli = Cli::parse();
    // `ctl` and `man` are split out first: neither may require a config file the
    // way the server does — --url/--token can supply everything the CLI needs,
    // and man generation runs in a build sandbox with no config at all.
    let command = match cli.command {
        Some(Command::Man { dir }) => return write_man_pages(&dir),
        Some(Command::Ctl { opts, command }) => {
            if let ctl::CtlCommand::Completions { shell } = command {
                ctl::print_completions(shell, &mut Cli::command());
                return Ok(());
            }
            let cfg = ctl_config(cli.config);
            return ctl::run(command, cfg, opts.url, opts.token, opts.json).await;
        }
        other => other.unwrap_or(Command::Serve),
    };

    let cfg_path = config::resolve_path(cli.config)?;
    let cfg = config::load(&cfg_path)?;

    match command {
        Command::Serve => serve(cfg, cfg_path).await,
        Command::Helper { mode } => match mode {
            HelperMode::Configtest => helper::configtest(&cfg).await,
            HelperMode::Apply => helper::apply(&cfg).await,
            HelperMode::EnableStreams => helper::enable_streams(&cfg).await,
        },
        Command::ResetPassword => reset_password(cfg),
        Command::Ctl { .. } | Command::Man { .. } => unreachable!("handled above"),
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
