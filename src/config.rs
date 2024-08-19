use std::{env, fmt, fs, io};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use clap::{value_parser, ValueEnum};
use log::LevelFilter;
use regex::{Captures, Regex, Replacer};
use serde::Deserialize;
use once_cell::sync::Lazy;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub log_level: Option<LogLevel>,
    pub github: GithubConfig,
    pub machine_defaults: Option<MachineDefaultsConfig>,
    pub machines: Vec<MachineConfig>,
}

impl TryFrom<&Path> for Config {
    type Error = ConfigError;

    fn try_from(config_file: &Path) -> Result<Self, Self::Error> {
        let parsed_config: Config = match fs::read_to_string(config_file) {
            Ok(content) => {
                match serde_yaml_ng::from_str(content.as_str()) {
                    Ok(config) => {
                        Ok(config)
                    }
                    Err(cause) => {
                        Err(ConfigError::ParseFailure {
                            path: config_file.to_str().unwrap().to_string(),
                            cause,
                        })
                    }
                }
            }
            Err(cause) => {
                Err(ConfigError::ReadFailure {
                    path: config_file.to_str().unwrap().to_string(),
                    cause,
                })
            }
        }?;

        let config_dir = {
            let mut buf = PathBuf::from(config_file);
            buf.pop();
            buf
        };

        // TODO: Post-process the parsed configuration.
        // TODO: Improve resolve_config_value() so that:
        // - there's no need to specify config_dir every time.
        // - a caller can specify an Option<String> or an Option<&str>.

        let resolved_config = Config {
            log_level: parsed_config.log_level.or(Some(LogLevel::Info)),
            github: GithubConfig {
                personal_access_token: resolve_config_value(
                    config_dir.as_path(),
                    parsed_config.github.personal_access_token.as_str())?,
                runner: parsed_config.github.runner,
            },
            machine_defaults: parsed_config.machine_defaults,
            machines: parsed_config.machines,
        };

        // TODO: Merge MachineDefaultsConfig into MachineConfigs.
        // TODO: Validate the final configuration
        // - SSH authentication settings

        Ok(resolved_config)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogLevel {
    #[serde(rename = "trace")]
    Trace,
    #[serde(rename = "debug")]
    Debug,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "off")]
    Off,
}

impl LogLevel {
    pub fn to_level_filter(self) -> LevelFilter {
        let level_str = format!("{:?}", self);
        LevelFilter::from_str(level_str.as_str())
            .expect("Failed to convert LogLevel into LevelFilter")
    }
}

#[derive(Deserialize)]
pub struct GithubConfig {
    pub personal_access_token: String,
    pub runner: GithubRunnerConfig,
}

impl fmt::Debug for GithubConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "GithubConfig {{ personal_access_token: ")?;

        if self.personal_access_token.len() < 8 {
            write!(f, "[REDACTED]")?
        } else {
            write!(f, "{}...", &self.personal_access_token[..8])?
        }

        write!(f, ", runner: {:?} }}", self.runner)
    }
}

#[derive(Debug, Deserialize)]
pub struct GithubRunnerConfig {
    pub name_prefix: Option<String>,
    pub scope: Option<String>,
    pub repo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MachineDefaultsConfig {
    pub ssh: Option<SshConfig>,
    pub runners: Option<RunnersConfig>,
}

#[derive(Debug, Deserialize)]
pub struct MachineConfig {
    pub id: Option<String>,
    pub ssh: Option<SshConfig>,
    pub runners: Option<RunnersConfig>,
}

#[derive(Deserialize)]
pub struct SshConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub fingerprint: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub private_key_passphrase: Option<String>,
}

impl fmt::Debug for SshConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "SshConfig {{ host: {:?}, port: {:?}, fingerprint: {:?}, user: {:?}, password: {}, ",
            self.host, self.port, self.fingerprint, self.user,
            match self.password {
                Some(_) => "[REDACTED]",
                None => "None",
            })?;

        write!(f, "private_key: ")?;
        match &self.private_key {
            Some(key) => if key.len() < 16 {
                write!(f, "[REDACTED]")?
            } else {
                write!(f, "{}...", &key[..16])?
            },
            None => write!(f, "None")?
        };

        write!(
            f, ", private_key_passphrase: {} }}",
            match self.private_key_passphrase {
                Some(_) => "[REDACTED]",
                None => "None",
            })
    }
}

#[derive(Debug, Deserialize)]
pub struct RunnersConfig {
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub idle_timeout: Option<String>,
}

#[derive(Debug)]
pub enum ConfigError {
    ReadFailure { path: String, cause: io::Error },
    ParseFailure { path: String, cause: serde_yaml_ng::Error },
    UnresolvedEnvironmentVariable { name: String, cause: env::VarError },
    UnresolvedFileVariable { path: String, cause: io::Error }
}

fn resolve_config_value(config_dir: &Path, input: &str) -> Result<String, ConfigError> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\$\$)|\$\{(file:)?([^}]+)}").unwrap());
    let config_error_ref: RefCell<Option<ConfigError>> = RefCell::new(None);
    let resolved_value = RE.replace_all(
        input,
        ConfigVariableResolver {
            config_dir,
            config_error_ref: &config_error_ref
        }).to_string();

    if let Some(config_error) = config_error_ref.take() {
        Err(config_error)
    } else {
        Ok(resolved_value)
    }
}

struct ConfigVariableResolver<'a> {
    config_dir: &'a Path,
    config_error_ref: &'a RefCell<Option<ConfigError>>,
}

impl Replacer for ConfigVariableResolver<'_> {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        // Replace '$$' with '$'.
        if caps.get(1).is_some() {
            dst.push_str("$");
            return;
        }


        // Replace ${...} with the environment variable value or the file content.
        let name = caps.get(3).unwrap().as_str();
        match caps.get(2) {
            Some(_) => self.append_file(name, dst),
            None => self.append_env_var(name, dst)
        }
    }
}

impl ConfigVariableResolver<'_> {
    fn append_env_var(&mut self, name: &str, dst: &mut String) {
        match env::var(name) {
            Ok(value) => {
                dst.push_str(value.as_str());
            }
            Err(cause) => {
                self.set_config_error(ConfigError::UnresolvedEnvironmentVariable {
                    name: String::from(name),
                    cause,
                });
            }
        }
    }

    fn append_file(&mut self, path: &str, dst: &mut String) {
        let path = {
            let mut buf = PathBuf::from(self.config_dir);
            buf.push(path);
            buf
        };

        match fs::read_to_string(path.as_path()) {
            Ok(content) => {
                dst.push_str(content.trim_end());
            },
            Err(cause) => {
                self.set_config_error(ConfigError::UnresolvedFileVariable {
                    path: path.to_str().unwrap().to_string(),
                    cause,
                });
            }
        }
    }

    fn set_config_error(&mut self, config_error: ConfigError) {
        let cell = self.config_error_ref;
        if cell.borrow().is_none() {
            cell.replace(Some(config_error));
        }
    }
}
