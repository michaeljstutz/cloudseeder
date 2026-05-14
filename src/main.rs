use clap::Parser;
use cloudseeder::{PrefixSource, Settings};
use std::path::PathBuf;

/// Dynamic template-based server for Ubuntu autoinstall and Red Hat kickstart configs.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to the cloudseeder.toml config file. Defaults to ./cloudseeder.toml if present;
    /// built-in defaults apply otherwise.
    #[arg(long, env = "CLOUDSEEDER_CONFIG", value_name = "PATH")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let settings = Settings::load(cli.config.as_deref()).unwrap_or_else(|e| {
        eprintln!("cloudseeder: {e}");
        std::process::exit(2);
    });

    match settings.prefix_source {
        PrefixSource::Generated => {
            tracing::info!(
                prefix = %settings.prefix,
                "prefix auto-generated (set `prefix = \"...\"` in cloudseeder.toml to keep it stable across restarts)"
            );
            tracing::info!(url = %format!("http://{}/{}/", settings.addr, settings.prefix), "ready");
        }
        PrefixSource::Config => {
            tracing::info!(path = ?settings.config_path, "config loaded");
            tracing::info!(addr = %settings.addr, "ready");
        }
    }

    cloudseeder::serve(settings.addr, &settings.prefix).await
}
