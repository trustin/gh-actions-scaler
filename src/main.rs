mod config;
mod github;
mod machine;

use std::error::Error;
use std::path::PathBuf;
use std::process::exit;

use crate::config::{Config, ConfigError, LogLevel};
use crate::github::GithubClient;
use crate::machine::Machine;
use clap::Parser;
use log::{debug, error, info, LevelFilter};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Sets a custom config file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Sets the log level.
    #[arg(short, long, value_name = "LEVEL")]
    log_level: Option<LogLevel>,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Determine the path of the configuration file.
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(|| {
        if let Some(user_config_dir) = dirs::config_dir() {
            let mut buf = PathBuf::new();
            buf.push(user_config_dir);
            buf.push("gh-actions-scaler");
            buf.push("config.yaml");
            buf
        } else {
            eprintln!("Failed to determine the default config file location.");
            eprintln!("Use '--config' option instead.");
            exit(1);
        }
    });

    pretty_env_logger::formatted_timed_builder()
        .default_format()
        .format_module_path(false)
        .format_target(false)
        // Make sure the messages at any log levels are preserved,
        // so that we can dynamically adjust the log level after loading the configuration.
        .filter_level(LevelFilter::Trace)
        .init();

    // Start with INFO or CLI-provided level.
    log::set_max_level(cli.log_level.unwrap_or(LogLevel::Info).to_level_filter());

    info!("Using the configuration at: {}", config_path.display());
    let config = match Config::try_from(config_path.as_path()) {
        Ok(config) => config,
        Err(err) => match err {
            ConfigError::ReadFailure { path, cause } => {
                error!(
                    "Failed to read the configuration file: {} ({})",
                    path, cause
                );
                exit(1);
            }
            ConfigError::ParseFailure { path, cause } => {
                error!(
                    "Failed to parse the configuration file: {} ({})",
                    path, cause
                );
                exit(1);
            }
            ConfigError::UnresolvedEnvironmentVariable { name, cause } => {
                error!(
                    "Failed to resolve an environment variable: {} ({})",
                    name, cause
                );
                exit(1);
            }
            ConfigError::UnresolvedFileVariable { path, cause } => {
                error!("Failed to resolve an external file: {} ({})", path, cause);
                exit(1);
            }
            ConfigError::ValidationFailure { message } => {
                error!("Invalid configuration: {}", message);
                exit(1);
            }
        },
    };

    // Use the log level specified in the configuration file, if CLI log level was not specified.
    if cli.log_level.is_none() {
        log::set_max_level(config.log_level.to_level_filter());
    }

    debug!("Deserialized configuration: {:#?}", config);

    let github_client = GithubClient::new(&config.github);
    let queued_runs = github_client.fetch_queued_workflow_runs()?;

    info!("{:#?}", queued_runs);

    let first_machine: Machine = Machine::new_with_session(&config.machines[0]).map_err(|e| {
        error!("Failed to connect session: {}", e);
        e
    })?;
    debug!("{:#?}", first_machine.fetch_runners()?);

    for run in queued_runs {
        info!("Starting a new runner for: {}", run.url);
        first_machine.start_runner(&config)?;
        debug!("{:#?}", first_machine.fetch_runners()?);
    }

    Ok(())
}
