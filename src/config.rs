mod resolver;

use crate::config::resolver::ConfigResolver;
use clap::ValueEnum;
use log::warn;
use log::LevelFilter;
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{env, fmt, fs, io};

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub log_level: LogLevel,
    pub github: GithubConfig,
    #[serde(default)]
    pub machine_defaults: MachineDefaultsConfig,
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
        let resolved_machine_defaults =
            Self::resolve_machine_defaults_config(&parsed_config.machine_defaults, &resolver)?;
        Ok(Config {
            log_level: parsed_config.log_level,
            github: Self::resolve_github_config(&parsed_config.github, &resolver)?,
            machines: Self::resolve_machine_configs(
                &resolved_machine_defaults,
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
        c: &MachineDefaultsConfig,
        r: &ConfigResolver,
    ) -> Result<MachineDefaultsConfig, ConfigError> {
        Ok(MachineDefaultsConfig {
            ssh: Self::resolve_default_ssh_config(&c.ssh, r)?,
            runners: RunnersConfig { max: c.runners.max },
        })
    }

    fn resolve_default_ssh_config(
        c: &SshConfig,
        r: &ConfigResolver,
    ) -> Result<SshConfig, ConfigError> {
        if !c.fingerprint.is_empty() {
            warn!("'fingerprint' in 'machine_defaults' will be ignored.");
        }

        Ok(SshConfig {
            host: r.resolve(&c.host)?,
            port: c.port,
            fingerprint: "".to_string(),
            username: r.resolve(&c.username)?,
            password: r.resolve(&c.password)?,
            private_key: r.resolve(&c.private_key)?,
            private_key_passphrase: r.resolve(&c.private_key_passphrase)?,
        })
    }

    fn resolve_machine_configs(
        defaults: &MachineDefaultsConfig,
        cfgs: &Vec<MachineConfig>,
        r: &ConfigResolver,
    ) -> Result<Vec<MachineConfig>, ConfigError> {
        let mut out: Vec<MachineConfig> = vec![];
        let mut id_generator = MachineIdGenerator::new(cfgs)?;
        for c in cfgs {
            let id = id_generator.generate(c, r)?;
            let ssh = Self::resolve_ssh_config(&id, &defaults.ssh, &c.ssh, r)?;
            let runners = Self::resolve_runners_config(&defaults.runners, &c.runners)?;
            out.push(MachineConfig { id, ssh, runners })
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
        machine_id: &str,
        defaults: &SshConfig,
        c: &SshConfig,
        r: &ConfigResolver,
    ) -> Result<SshConfig, ConfigError> {
        // Choose the password or private key in the following order of preferences:
        // 1) A per-machine private key
        // 2) A per-machine password
        // 3) The default private key
        // 4) The default password
        let password_or_private_key: (&str, &str, &str) = {
            if !c.private_key.is_empty() {
                if !c.password.is_empty() {
                    warn!(
                        "'password' will be ignored for machine '{}' in favor of 'private_key'.",
                        machine_id
                    );
                }
                (
                    "",
                    c.private_key.as_str(),
                    c.private_key_passphrase.as_str(),
                )
            } else if !c.password.is_empty() {
                (c.password.as_str(), "", "")
            } else if !defaults.private_key.is_empty() {
                (
                    "",
                    defaults.private_key.as_str(),
                    defaults.private_key_passphrase.as_str(),
                )
            } else {
                (defaults.password.as_str(), "", "")
            }
        };

        let resolved = SshConfig {
            host: r.resolve_or_else(&c.host, || {
                let fallback = defaults.host.clone();
                if fallback.is_empty() {
                    Err(ConfigError::ValidationFailure {
                        message: format!("'host' must be specified for machine '{}'.", machine_id),
                    })
                } else {
                    Ok(fallback)
                }
            })?,
            port: if c.port != 0 {
                c.port
            } else if defaults.port != 0 {
                defaults.port
            } else {
                22
            },
            // Don't look up the defaults because every machine has its own fingerprint.
            fingerprint: r.resolve(&c.fingerprint)?,
            username: r.resolve_or_else(&c.username, || {
                let fallback = defaults.username.clone();
                if fallback.is_empty() {
                    Err(ConfigError::ValidationFailure {
                        message: format!(
                            "'username' must be specified for machine '{}'.",
                            machine_id
                        ),
                    })
                } else {
                    Ok(fallback)
                }
            })?,
            password: r.resolve(password_or_private_key.0)?,
            private_key: r.resolve(password_or_private_key.1)?,
            private_key_passphrase: r.resolve(password_or_private_key.2)?,
        };

        // Ensure password or private key is specified.
        if resolved.password.is_empty() && resolved.private_key.is_empty() {
            return Err(ConfigError::ValidationFailure {
                message: format!(
                    "'password' or 'private_key' must be specified for machine '{}'.",
                    machine_id
                ),
            });
        }

        Ok(resolved)
    }

    fn resolve_runners_config(
        defaults: &RunnersConfig,
        c: &RunnersConfig,
    ) -> Result<RunnersConfig, ConfigError> {
        let default_max_runners = 16;
        Ok(RunnersConfig {
            max: if c.max != 0 {
                c.max
            } else if defaults.max != 0 {
                defaults.max
            } else {
                default_max_runners
            },
        })
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub enum LogLevel {
    #[serde(rename = "trace")]
    Trace,
    #[serde(rename = "debug")]
    Debug,
    #[serde(rename = "info")]
    #[default]
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

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    #[serde(default)]
    pub personal_access_token: String,
    pub runners: GithubRunnerConfig,
}

impl Debug for GithubConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("GithubConfig")
            .field(
                "personal_access_token",
                mask_credential(&self.personal_access_token),
            )
            .field("runners", &self.runners)
            .finish()
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GithubRunnerConfig {
    #[serde(default = "default_github_runner_name_prefix")]
    pub name_prefix: String,
    #[serde(default = "default_github_runner_scope")]
    pub scope: String,
    #[serde(default)]
    pub repo_url: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct MachineDefaultsConfig {
    #[serde(default)]
    pub ssh: SshConfig,
    #[serde(default)]
    pub runners: RunnersConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MachineConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub ssh: SshConfig,
    #[serde(default)]
    pub runners: RunnersConfig,
}

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SshConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub private_key_passphrase: String,
}

impl Default for SshConfig {
    fn default() -> Self {
        SshConfig {
            host: "".to_string(),
            port: 0,
            fingerprint: "".to_string(),
            username: "".to_string(),
            password: "".to_string(),
            private_key: "".to_string(),
            private_key_passphrase: "".to_string(),
        }
    }
}

impl Debug for SshConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SshConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("fingerprint", &self.fingerprint)
            .field("username", &self.username)
            .field("password", mask_credential(&self.password))
            .field("private_key", mask_credential(&self.private_key))
            .field(
                "private_key_passphrase",
                mask_credential(&self.private_key_passphrase),
            )
            .finish()
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct RunnersConfig {
    #[serde(default)]
    pub max: u32,
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

fn mask_credential(value: &str) -> &dyn Debug {
    if value.is_empty() {
        &""
    } else {
        &"[REDACTED]"
    }
}

// TODO: Use field_with() and write_masked_credential_with_preview() when field_with() becomes stable.
//       https://github.com/rust-lang/rust/issues/117729
// e.g.
// formatter
//   .debug_struct("GithubConfig")
//   .field_with("personal_access_token", |f| write_masked_credential_with_preview(f, ...))
//   .finish()
//
#[allow(dead_code)]
fn write_masked_credential_with_preview(f: &mut Formatter, value: &str) -> fmt::Result {
    if value.is_empty() {
        f.write_str("[UNSPECIFIED]")
    } else if value.len() < 8 {
        f.write_str("[REDACTED]")
    } else {
        f.write_str(&value[..8])
    }
}

// Default value functions for serde

fn default_github_runner_name_prefix() -> String {
    "runner".to_string()
}

fn default_github_runner_scope() -> String {
    "repo".to_string()
}
