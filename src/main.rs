#[macro_use]
extern crate log;
extern crate pretty_env_logger;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use clap::{Parser, ValueEnum};
use dirs;
use log::LevelFilter;


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

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

impl LogLevel {
    fn to_level_filter(self) -> LevelFilter {
        let level_str = format!("{:?}", self);
        LevelFilter::from_str(level_str.as_str())
            .expect("Failed to convert LogLevel into LevelFilter")
    }
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
        // TODO: Configure the log level from the configuration file.
        .filter_level(cli.log_level.unwrap_or(LogLevel::Info).to_level_filter())
        .init();

    info!("Using the configuration at: {}", config_path.display());
}
