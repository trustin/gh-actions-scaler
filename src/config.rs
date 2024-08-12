use std::fmt;
use std::str::FromStr;
use clap::ValueEnum;
use log::LevelFilter;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub(crate) log_level: Option<LogLevel>,
    github: GithubConfig,
    machine_defaults: Option<MachineDefaultsConfig>,
    machines: Vec<MachineConfig>,
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

#[derive(Debug, Deserialize)]
pub struct GithubConfig {
    personal_access_token: String,
    runner: GithubRunnerConfig,
}

#[derive(Debug, Deserialize)]
pub struct GithubRunnerConfig {
    name_prefix: Option<String>,
    scope: Option<String>,
    repo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MachineDefaultsConfig {
    ssh: Option<SshConfig>,
    runners: Option<RunnersConfig>,
}

#[derive(Debug, Deserialize)]
pub struct MachineConfig {
    id: Option<String>,
    ssh: Option<SshConfig>,
    runners: Option<RunnersConfig>,
}

#[derive(Deserialize)]
pub struct SshConfig {
    host: Option<String>,
    port: Option<u16>,
    fingerprint: Option<String>,
    user: Option<String>,
    password: Option<String>,
    private_key: Option<String>,
    private_key_passphrase: Option<String>,
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
    min: Option<u32>,
    max: Option<u32>,
    idle_timeout: Option<String>,
}
