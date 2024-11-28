use crate::config::{Config, MachineConfig};
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

    fn connect_session(&mut self) -> Result<(), Box<dyn Error>> {
        let config = &self.config;
        let socket_addr = &self.socket_addr;

        // Connect to the local SSH server
        debug!("[{}] Making a connection attempt ..", socket_addr);
        let tcp = TcpStream::connect_timeout(socket_addr, Duration::from_secs(30))?;
        debug!(
            "[{}] Connection established; creating an SSH session ..",
            socket_addr
        );
        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;
        debug!(
            "[{}] SSH session established; authenticating ..",
            socket_addr
        );
        if config.ssh.password.is_empty() {
            debug!("[{}] Using private key authentication", socket_addr);
            session.userauth_pubkey_memory(
                &config.ssh.username,
                None,
                &config.ssh.private_key,
                self.passphrase_opt(),
            )?;
        } else {
            debug!("[{}] Using password authentication", socket_addr);
            session.userauth_password(&config.ssh.username, &config.ssh.password)?;
        }

        if !session.authenticated() {
            return Err("Authentication failed".into());
        }

        self.session = Some(session);
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

    pub fn start_runner(&self, config: &Config, run_url: &str) -> Result<(), Box<dyn Error>> {
        let session = self.get_session()?;

        // TODO: Make the image URL configurable.
        const IMAGE: &str = "ghcr.io/myoung34/docker-github-actions-runner:ubuntu-focal";

        // FIXME(trustin): Pull only once a day.
        //                 Keep the timestamp in ~/.cache/gh-actions-scaler (or $XDG_CACHE_HOME/...)
        info!(
            "[{}] Pulling the container image '{}' ..",
            &self.socket_addr, IMAGE
        );

        Self::ssh_exec(
            &self.socket_addr,
            session,
            &["docker", "image", "pull", IMAGE],
        )?;

        info!("[{}] Pulled the container image", &self.socket_addr);

        // FIXME(trustin): Specify a unique yet identifiable container name.
        //                 Use `docker container rename <container_id> github-self-hosted-runner-<container_id>
        info!(
            "[{}] Creating and starting a new container ..",
            &self.socket_addr
        );
        let container_id = Self::ssh_exec_with_env(
            &self.socket_addr,
            session,
            &hashmap! {
                "ACCESS_TOKEN" => config.github.personal_access_token.as_str(),
            },
            &vec![
                "docker",
                "container",
                "run",
                "--detach",
                "--label",
                "github-self-hosted-runner",
                "--label",
                &format!("github-workflow-run-url={}", run_url),
                "--env",
                "ACCESS_TOKEN",
                "--env",
                &format!("REPO_URL={}", config.github.runners.repo_url),
                "--env",
                &format!("RUNNER_NAME_PREFIX={}", config.github.runners.name_prefix),
                "--env",
                &format!("RUNNER_SCOPE={}", config.github.runners.scope),
                "--env",
                "EPHEMERAL=true",
                "--env",
                "UNSET_CONFIG_VARS=true",
                IMAGE,
            ],
        )?;
        info!(
            "[{}] Started a new container: {}",
            &self.socket_addr, container_id
        );

        Ok(())
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
        command: &Vec<&str>,
    ) -> Result<String, Box<dyn Error>> {
        let env_script_path = Self::ssh_generate_env_script(socket_addr, session, env)?;

        // Prepend the command that sources the environment variable script and removes it.
        let mut cmd_with_env = vec![".", &env_script_path, "&&", "rm", &env_script_path, "&&"];
        for arg in command {
            cmd_with_env.push(arg);
        }

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
            &["mktemp", "-t", "self-hosted-runner-env.XXXXXXXXXX"],
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

        Self::ssh_exec_noescape(socket_addr, session, cmd)?;
        Ok(env_script_path)
    }

    fn ssh_exec(
        socket_addr: &SocketAddr,
        session: &Session,
        command: &[&str],
    ) -> Result<String, Box<dyn Error>> {
        // Merge the arguments into a string while escaping if necessary.
        let mut cmd = String::new();
        for (i, arg) in command.iter().enumerate() {
            if i != 0 {
                cmd.push(' ');
            }
            cmd.push_str_escaped(arg);
        }
        Self::ssh_exec_noescape(socket_addr, session, cmd)
    }

    fn ssh_exec_noescape(
        socket_addr: &SocketAddr,
        session: &Session,
        cmd: String,
    ) -> Result<String, Box<dyn Error>> {
        let mut ch = session.channel_session()?;
        ch.exec(&cmd)?;

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

pub trait StringExt {
    fn push_str_escaped(&mut self, s: &str);
}

impl StringExt for String {
    fn push_str_escaped(&mut self, s: &str) {
        if !s.contains(['\'', '"', ' ', '\\']) {
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
