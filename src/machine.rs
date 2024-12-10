use crate::config::{Config, MachineConfig};
use chrono::{DateTime, Datelike, ParseResult, Utc};
use log::{debug, info};
use maplit::hashmap;
use ssh2::Session;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub struct Machine {
    config: MachineConfig,
    socket_addr: SocketAddr,
    session: Option<Session>,
}

impl Machine {
    pub fn new(config: &MachineConfig) -> Result<Self, Box<dyn Error>> {
        let cloned_config = config.clone();
        let socket_addr = SocketAddr::new(cloned_config.ssh.host.parse()?, cloned_config.ssh.port);
        Ok(Self {
            config: cloned_config,
            socket_addr,
            session: None,
        })
    }

    pub fn new_with_session(config: &MachineConfig) -> Result<Self, Box<dyn Error>> {
        let mut machine = Self::new(config)?;
        machine.connect_session()?;
        Ok(machine)
    }

    pub fn fetch_runners(&self) -> Result<Vec<RunnerInfo>, Box<dyn Error>> {
        let sess = self.get_session()?;

        info!("[{}] Retrieving the list of runners ..", &self.socket_addr);

        let mut cmd = String::new();
        cmd.push_str("docker container ls --all --no-trunc --filter ");
        cmd.push_str_escaped("label=github-self-hosted-runner");
        cmd.push_str(" --format {{.ID}} ");
        cmd.push_str("| xargs --no-run-if-empty docker container inspect --format ");
        cmd.push_str_escaped(
            "{{.ID}}|{{.State.Status}}|{{.Created}}|{{.State.StartedAt}}|{{.State.FinishedAt}}",
        );

        let output = Self::ssh_exec(&self.socket_addr, sess, &cmd)?;

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
        let sess = self.get_session()?;

        // TODO: Make the image URL configurable.
        const IMAGE: &str = "ghcr.io/myoung34/docker-github-actions-runner:ubuntu-focal";

        // FIXME(trustin): Pull only once a day.
        //                 Keep the timestamp in ~/.cache/gh-actions-scaler (or $XDG_CACHE_HOME/...)
        info!(
            "[{}] Pulling the container image '{}' ..",
            &self.socket_addr, IMAGE
        );
        let mut pull_cmd = String::new();
        pull_cmd.push_str("docker image pull ");
        pull_cmd.push_str_escaped(IMAGE);
        Self::ssh_exec(&self.socket_addr, sess, &pull_cmd)?;

        info!("[{}] Pulled the container image", &self.socket_addr);

        // FIXME(trustin): Specify a unique yet identifiable container name.
        //                 Use `docker container rename <container_id> github-self-hosted-runner-<container_id>
        info!(
            "[{}] Creating and starting a new container ..",
            &self.socket_addr
        );
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
            &self.socket_addr,
            sess,
            &hashmap! {
                "ACCESS_TOKEN" => config.github.personal_access_token.as_str(),
            },
            &run_cmd,
        )?;
        info!(
            "[{}] Started a new container: {}",
            &self.socket_addr, container_id
        );

        Ok(())
    }

    fn connect_session(&mut self) -> Result<(), Box<dyn Error>> {
        let config = &self.config;
        let socket_addr = &self.socket_addr;

        // Connect to the SSH server
        debug!("[{}] Making a connection attempt ..", socket_addr);
        let tcp = TcpStream::connect_timeout(socket_addr, Duration::from_secs(30))?;
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
        if config.ssh.password.is_empty() {
            debug!("[{}] Using private key authentication", socket_addr);
            sess.userauth_pubkey_memory(
                &config.ssh.username,
                None,
                &config.ssh.private_key,
                self.passphrase_opt(),
            )?;
        } else {
            debug!("[{}] Using password authentication", socket_addr);
            sess.userauth_password(&config.ssh.username, &config.ssh.password)?;
        }

        if !sess.authenticated() {
            return Err("Authentication failed".into());
        }

        self.session = Some(sess);
        Ok(())
        // TODO If the cache file does not exist, create a session only once.
        //      ~/.cache/gh-actions-scaler (or $XDG_CACHE_HOME/...)
    }

    fn get_session(&self) -> Result<&Session, Box<dyn Error>> {
        self.session.as_ref().ok_or_else(|| {
            let ssh = &self.config.ssh;
            format!(
                "[{}:{}] not connected to the machine yet. Try after SSH connect.",
                ssh.host, ssh.port
            )
            .into()
        })
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
        session: &Session,
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
        session: &Session,
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
        session: &Session,
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
