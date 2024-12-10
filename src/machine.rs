use crate::config::{Config, MachineConfig};
use chrono::{DateTime, Datelike, ParseResult, Utc};
use log::{debug, info, warn};
use maplit::hashmap;
use ssh2::Session;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct Machine {
    config: MachineConfig,
}

impl Machine {
    pub fn new(config: &MachineConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub fn fetch_runners(&self) -> Result<Vec<RunnerInfo>, Box<dyn Error>> {
        let (socket_addr, mut sess) = self.connect()?;

        info!("[{}] Retrieving the list of runners ..", socket_addr);

        let mut cmd = String::new();
        cmd.push_str("docker container ls --all --no-trunc --filter ");
        cmd.push_str_escaped("label=github-self-hosted-runner");
        cmd.push_str(" --format {{.ID}} ");
        cmd.push_str("| xargs --no-run-if-empty docker container inspect --format ");
        cmd.push_str_escaped(
            "{{.ID}}|{{.State.Status}}|{{.Created}}|{{.State.StartedAt}}|{{.State.FinishedAt}}",
        );

        let output = Self::ssh_exec(&socket_addr, &mut sess, &cmd)?;

        // Parse the output.
        let mut res: Vec<RunnerInfo> = vec![];
        for line in output.lines() {
            let fields: Vec<&str> = line.split(['|']).collect();
            res.push(RunnerInfo {
                container_id: fields[0].to_string(),
                container_state: ContainerState::from(fields[1]),
                created_at: Self::parse_timestamp(fields[2])?,
                started_at: Self::parse_timestamp_opt(fields[3])?,
                finished_at: Self::parse_timestamp_opt(fields[4])?,
            });
        }

        Ok(res)
    }

    fn parse_timestamp_opt(text: &str) -> ParseResult<Option<DateTime<Utc>>> {
        let timestamp = Self::parse_timestamp(text)?;
        if timestamp.year() > 1970 {
            Ok(Some(timestamp))
        } else {
            Ok(None)
        }
    }

    fn parse_timestamp(text: &str) -> ParseResult<DateTime<Utc>> {
        Ok(DateTime::parse_from_rfc3339(text)?.to_utc())
    }

    pub fn start_runner(&self, config: &Config) -> Result<(), Box<dyn Error>> {
        let (socket_addr, mut sess) = self.connect()?;

        let is_valid_cache_image = Self::is_valid_cache_image(&socket_addr, &mut sess)
            .unwrap_or_else(|err| {
                // FIXME(JopopScript) cant get current time or cant use cache version -> always image pulling
                //       for example cache file permission denied.
                warn!("[{}] Container image is unknown to be valid. so can't use cache image. err: {}", socket_addr, err);
                false
            });

        // TODO: Make the image URL configurable.
        const IMAGE: &str = "ghcr.io/myoung34/docker-github-actions-runner:ubuntu-focal";
        if !is_valid_cache_image {
            info!(
                "[{}] Pulling the container image '{}' ..",
                socket_addr, IMAGE
            );
            let mut pull_cmd = String::new();
            pull_cmd.push_str("docker image pull ");
            pull_cmd.push_str_escaped(IMAGE);
            Self::ssh_exec(&socket_addr, &mut sess, &pull_cmd)?;
            info!("[{}] Pulled the container image", socket_addr);
        } else {
            info!(
                "[{}] Cached container image '{}' already exists. no need to pull the image.",
                socket_addr, IMAGE
            );
        }

        // FIXME(trustin): Specify a unique yet identifiable container name.
        //                 Use `docker container rename <container_id> github-self-hosted-runner-<container_id>
        info!("[{}] Creating and starting a new container ..", socket_addr);
        let mut run_cmd = String::new();
        run_cmd.push_str("docker container run --detach --restart no --label ");
        run_cmd.push_str_escaped("github-self-hosted-runner");
        run_cmd.push_str(" --env ACCESS_TOKEN");
        run_cmd.push_str(" --env REPO_URL=");
        run_cmd.push_str_escaped(&config.github.runners.repo_url);
        run_cmd.push_str(" --env RUNNER_NAME_PREFIX=");
        run_cmd.push_str_escaped(&config.github.runners.name_prefix);
        run_cmd.push_str(" --env RUNNER_SCOPE=");
        run_cmd.push_str_escaped(&config.github.runners.scope);
        run_cmd.push_str(" --env EPHEMERAL=true");
        run_cmd.push_str(" --env UNSET_CONFIG_VARS=true ");
        run_cmd.push_str_escaped(IMAGE);

        let container_id = Self::ssh_exec_with_env(
            &socket_addr,
            &mut sess,
            &hashmap! {
                "ACCESS_TOKEN" => config.github.personal_access_token.as_str(),
            },
            &run_cmd,
        )?;

        let mut container_name = String::new();
        container_name.push_str("github-self-hosted-runner-");
        container_name.push_str(&container_id);

        let mut rename_cmd = String::new();
        rename_cmd.push_str("docker container rename ");
        rename_cmd.push_str(&container_id);
        rename_cmd.push_str(" ");
        rename_cmd.push_str_escaped(&container_name);
        Self::ssh_exec(&socket_addr, &mut sess, &rename_cmd)?;

        info!(
            "[{}] Started a new container: {}",
            socket_addr, container_name
        );
        Ok(())
    }

    fn connect(&self) -> Result<(SocketAddr, Session), Box<dyn Error>> {
        // Connect to the SSH server
        let socket_addr = SocketAddr::new(self.config.ssh.host.parse()?, self.config.ssh.port);
        debug!("[{}] Making a connection attempt ..", socket_addr);
        let tcp = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(30))?;
        debug!(
            "[{}] Connection established; creating an SSH session ..",
            socket_addr
        );
        let mut sess = Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;
        debug!(
            "[{}] SSH session established; authenticating ..",
            socket_addr
        );
        if self.config.ssh.password.is_empty() {
            debug!("[{}] Using private key authentication", socket_addr);
            sess.userauth_pubkey_memory(
                &self.config.ssh.username,
                None,
                &self.config.ssh.private_key,
                self.passphrase_opt(),
            )?;
        } else {
            debug!("[{}] Using password authentication", socket_addr);
            sess.userauth_password(&self.config.ssh.username, &self.config.ssh.password)?;
        }

        if !sess.authenticated() {
            return Err("Authentication failed".into());
        }

        Ok((socket_addr, sess))
    }

    /// Returns cache container image is valid
    /// # Returns
    ///
    /// * `Ok(true)` - Container image valid and has not expired
    /// * `Ok(false)`, `Error` - Container image is no longer valid.
    fn is_valid_cache_image(
        socket_addr: &SocketAddr,
        sess: &mut Session,
    ) -> Result<bool, Box<dyn Error>> {
        let dir = Self::ssh_exec(
            socket_addr,
            sess,
            &["echo", "${XDG_CACHE_HOME:-$HOME/.cache}"],
        )?;
        let file_name = "gh-actions-scaler";
        let mut pull_path = String::new();
        pull_path.push_str(&dir);
        pull_path.push('/');
        pull_path.push_str(file_name);

        let now_version = Self::now_cache_version()?;

        let is_exist_file = Self::ssh_exec(socket_addr, sess, &["test", "-f", &pull_path])
            .map(|_| true)
            .unwrap_or(false);

        if !is_exist_file {
            Self::ssh_exec(socket_addr, sess, &["mkdir", "-p", &dir])?;
            Self::ssh_exec(socket_addr, sess, &["echo", &now_version, ">>", &pull_path])?;
            return Ok(false);
        }

        let cached_version = Self::ssh_exec(socket_addr, sess, &["cat", &pull_path])?;
        let is_valid = cached_version == now_version;
        if !is_valid {
            Self::ssh_exec(socket_addr, sess, &["echo", &now_version, ">>", &pull_path])?;
        }
        Ok(is_valid)
    }

    fn now_cache_version() -> Result<String, Box<dyn Error>> {
        let epoch_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())?;

        //TODO(JopopScript) convert date format yyyyMMdd ex) "20241125"
        let seconds_a_day = 86400; // 60 * 60 * 24 = 86400
        let days_since_epoch = epoch_seconds / seconds_a_day;
        let cache_version = days_since_epoch.to_string();
        Ok(cache_version)
    }

    fn passphrase_opt(&self) -> Option<&str> {
        let passphrase = &self.config.ssh.private_key_passphrase;
        if passphrase.is_empty() {
            None
        } else {
            Some(passphrase)
        }
    }

    fn ssh_exec_with_env(
        socket_addr: &SocketAddr,
        session: &mut Session,
        env: &HashMap<&str, &str>,
        command: &str,
    ) -> Result<String, Box<dyn Error>> {
        let env_script_path = Self::ssh_generate_env_script(socket_addr, session, env)?;

        // Prepend the command that sources the environment variable script and removes it.
        let mut cmd_with_env = String::new();
        cmd_with_env.push_str(". ");
        cmd_with_env.push_str_escaped(&env_script_path);
        cmd_with_env.push_str(" && rm ");
        cmd_with_env.push_str_escaped(&env_script_path);
        cmd_with_env.push_str(" && ");
        cmd_with_env.push_str(command);

        Self::ssh_exec(socket_addr, session, &cmd_with_env)
    }

    fn ssh_generate_env_script(
        socket_addr: &SocketAddr,
        session: &mut Session,
        env: &HashMap<&str, &str>,
    ) -> Result<String, Box<dyn Error>> {
        let env_script_path = Self::ssh_exec(
            socket_addr,
            session,
            "mktemp -t github-self-hosted-runner-env.XXXXXXXXXX",
        )?;

        let mut cmd = String::new();
        cmd.push_str("cat <<======== >");
        cmd.push_str_escaped(&env_script_path);
        cmd.push('\n');

        for kv in env {
            // KEY=VALUE
            cmd.push_str_escaped(kv.0);
            cmd.push('=');
            cmd.push_str_escaped(kv.1);
            cmd.push('\n');

            // export KEY
            cmd.push_str("export ");
            cmd.push_str_escaped(kv.0);
            cmd.push('\n');
        }

        cmd.push_str("========\n");

        Self::ssh_exec(socket_addr, session, &cmd)?;
        Ok(env_script_path)
    }

    fn ssh_exec(
        socket_addr: &SocketAddr,
        session: &mut Session,
        cmd: &str,
    ) -> Result<String, Box<dyn Error>> {
        let mut ch = session.channel_session()?;
        ch.exec(cmd)?;

        let mut stdout = String::new();
        let mut stderr = String::new();
        ch.read_to_string(&mut stdout)?;
        ch.stderr().read_to_string(&mut stderr)?;
        ch.wait_close()?;

        let exit_code = ch.exit_status()?;
        if exit_code == 0 {
            Ok(stdout.trim().to_string())
        } else {
            let mut indented_out: String =
                String::with_capacity((stdout.len() + stderr.len()) * 3 / 2);
            write!(
                indented_out,
                "[{}] Failed to execute the command:\n\n    {}\n\nExit code: {}",
                socket_addr, cmd, exit_code
            )?;

            if !stdout.is_empty() {
                write!(indented_out, "\nStandard output:\n\n")?;
                for line in stdout.lines() {
                    indented_out.push_str("    ");
                    indented_out.push_str(line);
                    indented_out.push('\n');
                }
            }

            if !stderr.is_empty() {
                write!(indented_out, "\nStandard error:\n\n")?;
                for line in stderr.lines() {
                    indented_out.push_str("    ");
                    indented_out.push_str(line);
                    indented_out.push('\n');
                }
            }

            Err(indented_out.into())
        }
    }
}

#[derive(Debug)]
pub struct RunnerInfo {
    container_id: String,
    container_state: ContainerState,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub enum ContainerState {
    Created,
    Restarting,
    Running,
    Removing,
    Paused,
    Exited,
    Dead,
    Unknown(String),
}

impl From<&str> for ContainerState {
    fn from(value: &str) -> Self {
        match value {
            "created" => ContainerState::Created,
            "restarting" => ContainerState::Restarting,
            "running" => ContainerState::Running,
            "removing" => ContainerState::Removing,
            "paused" => ContainerState::Paused,
            "exited" => ContainerState::Exited,
            "dead" => ContainerState::Dead,
            _ => ContainerState::Unknown(value.to_string()),
        }
    }
}

pub trait StringExt {
    fn push_str_escaped(&mut self, s: &str);
}

impl StringExt for String {
    fn push_str_escaped(&mut self, s: &str) {
        if !s.contains([
            '\'', '"', ' ', '\\', '|', '&', '!', ';', '$', '(', ')', '[', ']', '{', '}', '<', '>',
            '#', '`',
        ]) {
            // No need to escape
            self.push_str(s);
            return;
        }

        self.push('"');
        for ch in s.chars() {
            match ch {
                '"' => self.push_str("\\\""),
                '\\' => self.push_str("\\\\"),
                _ => self.push(ch),
            }
        }
        self.push('"');
    }
}
