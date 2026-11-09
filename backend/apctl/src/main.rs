//! The standalone `apctl` binary — Linux, macOS and Windows, arm64 and x86_64.
//!
//! All the behaviour lives in the library so `angie-panel ctl` can share it;
//! this is only the entry point.

use std::path::PathBuf;

use clap::{CommandFactory, Parser};

#[derive(Parser)]
#[command(
    name = "apctl",
    version,
    about = "Operator CLI for the Angie panel",
    long_about = "Operator CLI for the Angie panel.\n\n\
                  On the panel host it needs no configuration: it reads the \
                  machine-local token from the data directory. From anywhere \
                  else, pass --url and --token (or $ANGIE_PANEL_URL and \
                  $ANGIE_PANEL_TOKEN)."
)]
struct Cli {
    /// Config file (default: $ANGIE_PANEL_CONFIG, /etc/angie-panel.toml, ./angie-panel.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Panel URL (default: $ANGIE_PANEL_URL, else the configured bind address)
    #[arg(long, global = true)]
    url: Option<String>,
    /// API token (default: $ANGIE_PANEL_TOKEN, else the local token file)
    #[arg(long, global = true)]
    token: Option<String>,
    /// Print the raw API response instead of a human-readable summary
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: apctl::CtlCommand,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Handled before anything that needs a token or a reachable panel.
    match cli.command {
        apctl::CtlCommand::Completions { shell } => {
            apctl::print_completions(shell, &mut Cli::command());
            return Ok(());
        }
        apctl::CtlCommand::Man { ref dir } => {
            return apctl::write_man_page(dir, Cli::command());
        }
        _ => {}
    }

    let cfg = apctl::CliConfig::load(cli.config);
    let endpoint = apctl::Endpoint {
        url: cli.url,
        token: cli.token,
        bind_addr: cfg.bind_addr,
        port: cfg.port,
        data_dir: cfg.data_dir,
    };
    apctl::run(cli.command, endpoint, cli.json).await
}
