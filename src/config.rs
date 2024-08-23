mod resolver;

use crate::config::resolver::ConfigResolver;
use clap::ValueEnum;
use log::LevelFilter;
use serde::Deserialize;
use std::cmp::max;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{env, fmt, fs, io};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub log_level: Option<LogLevel>,
    pub github: GithubConfig,
    pub machine_defaults: Option<MachineDefaultsConfig>,
    pub machines: Vec<MachineConfig>,
}

impl Config {
    pub fn try_from<T: AsRef<Path>>(config_file: T) -> Result<Self, ConfigError> {
        let config_file = config_file.as_ref();
        let parsed_config: Config = match fs::read_to_string(config_file) {
            Ok(content) => match serde_yaml_ng::from_str(content.as_str()) {
                Ok(config) => Ok(config),
                Err(cause) => Err(ConfigError::ParseFailure {
                    path: config_file.to_str().unwrap().to_string(),
                    cause,
                }),
            },
            Err(cause) => Err(ConfigError::ReadFailure {
                path: config_file.to_str().unwrap().to_string(),
                cause,
            }),
        }?;

        let config_dir = {
            let mut buf = config_file.to_path_buf();
            buf.pop();
            if buf.components().next().is_none() {
                buf.push(PathBuf::from_str(".").unwrap());
            }
            buf
        };

        Self::resolve_config(&config_dir, &parsed_config)
    }
}

impl Config {
    fn resolve_config(config_dir: &PathBuf, parsed_config: &Config) -> Result<Config, ConfigError> {
        let resolver = resolver::ConfigResolver::from(&config_dir);
        let resolved_machine_defaults = Self::resolve_machine_defaults_config(
            parsed_config.machine_defaults.as_ref(),
            &resolver,
        )?;
        Ok(Config {
            log_level: parsed_config.log_level.or(Some(LogLevel::Info)),
            github: Self::resolve_github_config(&parsed_config.github, &resolver)?,
            machines: Self::resolve_machine_configs(
                resolved_machine_defaults.as_ref(),
                &parsed_config.machines,
                &resolver,
            )?,
            machine_defaults: resolved_machine_defaults,
        })
    }

    fn resolve_github_config(
        c: &GithubConfig,
        r: &ConfigResolver,
    ) -> Result<GithubConfig, ConfigError> {
        Ok(GithubConfig {
            personal_access_token: r.resolve(&c.personal_access_token)?,
            runner: GithubRunnerConfig {
                name_prefix: r.resolve_opt(&c.runner.name_prefix)?,
                scope: r.resolve_opt(&c.runner.scope)?,
                repo_url: r.resolve_opt(&c.runner.repo_url)?,
            },
        })

        // TODO Validate the configuration.
    }

    fn resolve_machine_defaults_config(
        c: Option<&MachineDefaultsConfig>,
        r: &ConfigResolver,
    ) -> Result<Option<MachineDefaultsConfig>, ConfigError> {
        Ok(match c {
            Some(c) => Some(MachineDefaultsConfig {
                ssh: Self::resolve_ssh_config(None, c.ssh.as_ref(), r)?,
                runners: Self::resolve_runners_config(None, c.runners.as_ref(), r)?,
            }),
            None => None,
        })
    }

    fn resolve_machine_configs(
        defaults: Option<&MachineDefaultsConfig>,
        cfgs: &Vec<MachineConfig>,
        r: &ConfigResolver,
    ) -> Result<Vec<MachineConfig>, ConfigError> {
        let mut out: Vec<MachineConfig> = vec![];
        match defaults {
            Some(d) => {
                for c in cfgs {
                    out.push(MachineConfig {
                        id: r.resolve_opt(&c.id)?,
                        ssh: Self::resolve_ssh_config(d.ssh.as_ref(), c.ssh.as_ref(), r)?,
                        runners: Self::resolve_runners_config(
                            d.runners.as_ref(),
                            c.runners.as_ref(),
                            r,
                        )?,
                    })
                }
            }
            None => {
                for c in cfgs {
                    out.push(MachineConfig {
                        id: r.resolve_opt(&c.id)?,
                        ssh: Self::resolve_ssh_config(None, c.ssh.as_ref(), r)?,
                        runners: Self::resolve_runners_config(None, c.runners.as_ref(), r)?,
                    })
                }
            }
        }

        Ok(out)
    }

    fn resolve_ssh_config(
        defaults: Option<&SshConfig>,
        c: Option<&SshConfig>,
        r: &ConfigResolver,
    ) -> Result<Option<SshConfig>, ConfigError> {
        Ok(match c {
            Some(c) => Some(SshConfig {
                host: r
                    .resolve_opt(&c.host)?
                    .or(defaults.and_then(|d| d.host.clone())),
                port: c.port.or(defaults.and_then(|d| d.port)).or(Some(22)),
                fingerprint: r
                    .resolve_opt(&c.fingerprint)?
                    .or(defaults.and_then(|d| d.fingerprint.clone())),
                username: r
                    .resolve_opt(&c.username)?
                    .or(defaults.and_then(|d| d.username.clone()))
                    .or(Some(whoami::username())),
                password: r
                    .resolve_opt(&c.password)?
                    .or(defaults.and_then(|d| d.password.clone())),
                private_key: r
                    .resolve_opt(&c.private_key)?
                    .or(defaults.and_then(|d| d.private_key.clone())),
                private_key_passphrase: r
                    .resolve_opt(&c.private_key_passphrase)?
                    .or(defaults.and_then(|d| d.private_key_passphrase.clone())),
            }),
            None => None, // TODO: Reject
        })

        // TODO Validate the configuration.
    }

    fn resolve_runners_config(
        defaults: Option<&RunnersConfig>,
        c: Option<&RunnersConfig>,
        r: &ConfigResolver,
    ) -> Result<Option<RunnersConfig>, ConfigError> {
        let default_min_runners = 1;
        let default_max_runners = 16;
        let default_idle_timeout = "1m";

        Ok(match c {
            Some(c) => {
                let min_runners = c.min.or(defaults.and_then(|d| d.min)).unwrap_or(1);
                let max_runners = c
                    .max
                    .or(defaults.and_then(|d| d.max))
                    .unwrap_or_else(|| max(min_runners, default_max_runners));

                Some(RunnersConfig {
                    min: Some(min_runners),
                    max: Some(max_runners),
                    idle_timeout: Some(
                        r.resolve_opt(&c.idle_timeout)?
                            .or(defaults.and_then(|d| d.idle_timeout.clone()))
                            .unwrap_or_else(|| default_idle_timeout.to_string()),
                    ),
                })
            }
            None => Some(RunnersConfig {
                min: Some(default_min_runners),
                max: Some(default_max_runners),
                idle_timeout: Some(default_idle_timeout.to_string()),
            }),
        })

        // TODO Validate the configuration.
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    pub personal_access_token: String,
    pub runners: GithubRunnerConfig,
}

impl fmt::Debug for GithubConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "GithubConfig {{ personal_access_token: ")?;

        if self.personal_access_token.len() < 8 {
            write!(f, "[REDACTED]")?
        } else {
            write!(f, "{}...", &self.personal_access_token[..8])?
        }

        write!(f, ", runners: {:?} }}", self.runners)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubRunnerConfig {
    pub name_prefix: Option<String>,
    pub scope: Option<String>,
    pub repo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineDefaultsConfig {
    pub ssh: Option<SshConfig>,
    pub runners: Option<RunnersConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineConfig {
    pub id: Option<String>,
    pub ssh: Option<SshConfig>,
    pub runners: Option<RunnersConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub fingerprint: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub private_key_passphrase: Option<String>,
}

impl fmt::Debug for SshConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "SshConfig {{ host: {:?}, port: {:?}, fingerprint: {:?}, username: {:?}, password: {}, ",
            self.host, self.port, self.fingerprint, self.username,
            match self.password {
                Some(_) => "[REDACTED]",
                None => "None",
            })?;

        write!(f, "private_key: ")?;
        match &self.private_key {
            Some(key) => {
                if key.len() < 16 {
                    write!(f, "[REDACTED]")?
                } else {
                    write!(f, "{}...", &key[..16])?
                }
            }
            None => write!(f, "None")?,
        };

        write!(
            f,
            ", private_key_passphrase: {} }}",
            match self.private_key_passphrase {
                Some(_) => "[REDACTED]",
                None => "None",
            }
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnersConfig {
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub idle_timeout: Option<String>,
}

#[derive(Debug)]
pub enum ConfigError {
    ReadFailure {
        path: String,
        cause: io::Error,
    },
    ParseFailure {
        path: String,
        cause: serde_yaml_ng::Error,
    },
    UnresolvedEnvironmentVariable {
        name: String,
        cause: env::VarError,
    },
    UnresolvedFileVariable {
        path: String,
        cause: io::Error,
    },
}
