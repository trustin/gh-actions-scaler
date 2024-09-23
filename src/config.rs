mod resolver;

use crate::config::resolver::ConfigResolver;
use clap::ValueEnum;
use log::LevelFilter;
use serde::Deserialize;
use std::cmp::max;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{env, fmt, fs, io};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    pub github: GithubConfig,
    pub machine_defaults: Option<MachineDefaultsConfig>,
    pub machines: Vec<MachineConfig>,
}

impl Config {
    pub fn try_from<T: AsRef<Path> + ?Sized>(config_file: &T) -> Result<Self, ConfigError> {
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
            log_level: parsed_config.log_level,
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
        let config = GithubConfig {
            personal_access_token: r.resolve(&c.personal_access_token)?,
            runners: GithubRunnerConfig {
                name_prefix: r.resolve(&c.runners.name_prefix)?,
                scope: r.resolve(&c.runners.scope)?,
                repo_url: r.resolve(&c.runners.repo_url)?,
            },
        };

        // Validate the personal access token.
        if config.personal_access_token.is_empty() {
            return Err(ConfigError::ValidationFailure {
                message: "An empty or missing value in 'github.personal_access_token'. A GitHub personal access token must start with 'ghp_'.".to_string(),
            });
        }
        if !config.personal_access_token.starts_with("ghp_") {
            return Err(ConfigError::ValidationFailure {
                message: "An invalid value in 'github.personal_access_token'. A GitHub personal access token must start with 'ghp_'.".to_string(),
            });
        }

        // Validate runner config.
        if config.runners.name_prefix.is_empty() {
            return Err(ConfigError::ValidationFailure {
                message: "An empty value in 'github.runners.name_prefix'.".to_string(),
            });
        }

        if config.runners.scope != "repo" {
            return Err(ConfigError::ValidationFailure {
                message: format!("An unsupported value '{}' in 'github.runners.scope'. 'repo' is the only supported value at the moment.", config.runners.scope)
            });
        }

        let repo_url = &config.runners.repo_url;
        if repo_url.is_empty() {
            return Err(ConfigError::ValidationFailure {
                message: "An empty or missing URL in 'github.runners.repo_url'.".to_string(),
            });
        }
        if !repo_url.starts_with("http://") && !repo_url.starts_with("https://") {
            return Err(ConfigError::ValidationFailure {
                message: format!("An invalid URL '{}' in github.runners.repo_url.", repo_url),
            });
        }

        Ok(config)
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
        let mut id_generator = MachineIdGenerator::new(cfgs)?;
        match defaults {
            Some(d) => {
                for c in cfgs {
                    out.push(MachineConfig {
                        id: id_generator.generate(c, r)?,
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
                        id: id_generator.generate(c, r)?,
                        ssh: Self::resolve_ssh_config(None, c.ssh.as_ref(), r)?,
                        runners: Self::resolve_runners_config(None, c.runners.as_ref(), r)?,
                    })
                }
            }
        }

        if out.is_empty() {
            Err(ConfigError::ValidationFailure {
                message: "There must be at least one machine in the configuration.".to_string(),
            })
        } else {
            out.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(out)
        }
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
    #[serde(default)]
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
    #[serde(default = "default_github_runner_name_prefix")]
    pub name_prefix: String,
    #[serde(default = "default_github_runner_scope")]
    pub scope: String,
    #[serde(default)]
    pub repo_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineDefaultsConfig {
    pub ssh: Option<SshConfig>,
    pub runners: Option<RunnersConfig>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MachineConfig {
    #[serde(default)]
    pub id: String,
    pub ssh: Option<SshConfig>,
    pub runners: Option<RunnersConfig>,
}

#[derive(Deserialize, PartialEq)]
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

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunnersConfig {
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub idle_timeout: Option<String>,
}

struct MachineIdGenerator {
    id_set: HashSet<String>,
    next_id: usize,
}

impl MachineIdGenerator {
    fn new<'a, T>(cfgs: &'a T) -> Result<MachineIdGenerator, ConfigError>
    where
        &'a T: IntoIterator<Item = &'a MachineConfig>,
    {
        let mut id_set = HashSet::<String>::new();
        for c in cfgs {
            if c.id.is_empty() {
                continue;
            }
            if !id_set.insert(c.id.clone()) {
                return Err(ConfigError::ValidationFailure {
                    message: format!("A duplicate machine ID '{}' was found.", c.id),
                });
            }
        }
        Ok(MachineIdGenerator { id_set, next_id: 1 })
    }

    fn generate(
        &mut self,
        cfg: &MachineConfig,
        resolver: &ConfigResolver,
    ) -> Result<String, ConfigError> {
        let specified_id = resolver.resolve(&cfg.id)?;
        // Use the specified ID if possible.
        if !specified_id.is_empty() {
            return Ok(specified_id);
        }

        // Generate a new ID otherwise.
        loop {
            let id = format!("machine-{}", self.next_id);
            if self.id_set.contains(&id) {
                self.next_id += 1;
            } else {
                self.id_set.insert(id.clone());
                return Ok(id);
            }
        }
    }
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
    ValidationFailure {
        message: String,
    },
}

// Default value functions for serde

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

fn default_github_runner_name_prefix() -> String {
    "runner".to_string()
}

fn default_github_runner_scope() -> String {
    "repo".to_string()
}
