use clap::{Parser, Subcommand};
use cloudseeder::{templates, PrefixSource, Settings};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

/// Dynamic template-based server for Ubuntu autoinstall and Red Hat kickstart configs.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to the cloudseeder.toml config file. Defaults to ./cloudseeder.toml if present;
    /// built-in defaults apply otherwise.
    #[arg(long, env = "CLOUDSEEDER_CONFIG", value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Render one template file to stdout without starting the HTTP server.
    Render {
        /// Template directory name under templates_dir.
        template: String,

        /// File to render: kickstart, user-data, or meta-data.
        file: String,

        /// Template variable as key=value. May be passed more than once.
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
    },
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

    match cli.command {
        Some(Command::Render {
            template,
            file,
            vars,
        }) => {
            let vars = parse_vars(vars).unwrap_or_else(|e| {
                eprintln!("cloudseeder: {e}");
                std::process::exit(2);
            });
            let body = templates::render_file(&settings.templates_dir, &template, &file, vars)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("cloudseeder: {e}");
                    std::process::exit(2);
                });
            std::io::stdout().write_all(&body)?;
            Ok(())
        }
        None => serve(settings).await,
    }
}

async fn serve(settings: Settings) -> std::io::Result<()> {
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

    cloudseeder::serve(settings.addr, &settings.prefix, settings.templates_dir).await
}

fn parse_vars(vars: Vec<String>) -> Result<HashMap<String, String>, String> {
    let mut parsed = HashMap::new();
    for var in vars {
        let Some((name, value)) = var.split_once('=') else {
            return Err(format!("invalid --var {var:?}: expected KEY=VALUE"));
        };
        templates::validate_var(name, value).map_err(|e| e.to_string())?;
        parsed.insert(name.to_string(), value.to_string());
    }
    Ok(parsed)
}
