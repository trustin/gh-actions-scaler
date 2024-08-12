mod config;

#[macro_use]
extern crate log;
extern crate pretty_env_logger;

use std::fs;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use clap::{Parser, ValueEnum};
use dirs;
use log::LevelFilter;
use crate::config::{Config, LogLevel};

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

fn main() {
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

    // TODO: Parse the configuration file into the config object.

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
    let config_str = fs::read_to_string(config_path).unwrap_or_else(|err| {
        error!("Failed to read the configuration file: {}", err);
        exit(1);
    });

    let config: Config = serde_yaml_ng::from_str(config_str.as_str()).unwrap_or_else(|err| {
        error!("Failed to parse the configuration: {}", err);
        exit(1);
    });

    // Adjust the log level if:
    // - A user did not override the log level with the CLI option; and
    // - A user specified the log level in the configuration.
    if cli.log_level.is_none() {
        if let Some(log_level) = config.log_level {
            log::set_max_level(log_level.to_level_filter());
        }
    }

    debug!("Deserialized configuration: {:?}", config);

    // TODO: Post-process the parsed configuration.
    // - "${ENV_VAR}"
    // - "${file:path/to/file}"
    // - "$${...}" should be interpreted into "${...}"
    // - "${..." (invalid syntax) should trigger failure.

    // TODO: Merge MachineDefaultsConfig into MachineConfigs.
    // TODO: Validate the final configuration
    // - SSH authentication settings
}
