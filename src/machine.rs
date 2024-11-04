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
}

impl Machine {
    pub fn new(config: &MachineConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub fn start_runner(&self, config: &Config, run_id: u32) -> Result<(), Box<dyn Error>> {
        // Connect to the local SSH server
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

        // TODO: Make the image URL configurable.
        const IMAGE: &str = "ghcr.io/myoung34/docker-github-actions-runner:ubuntu-focal";

        info!(
            "[{}] Pulling the container image '{}' ..",
            socket_addr, IMAGE
        );
        Self::ssh_exec(&socket_addr, &mut sess, &["docker", "pull", IMAGE])?;

        info!("[{}] Pulled the container image", socket_addr);

        info!("[{}] Creating and starting a new container ..", socket_addr);
        let container_id = Self::ssh_exec_with_env(
            &socket_addr,
            &mut sess,
            &hashmap! {
                "ACCESS_TOKEN" => config.github.personal_access_token.as_str(),
            },
            &vec![
                "docker",
                "run",
                // "--detach",
                "--label",
                "github-self-hosted-runner",
                "--label",
                format!("github-workflow-run-id={}", run_id).as_str(),
                "--env",
                "ACCESS_TOKEN",
                "--env",
                format!("REPO_URL={}", config.github.runners.repo_url).as_str(),
                "--env",
                format!("RUNNER_NAME_PREFIX={}", config.github.runners.name_prefix).as_str(),
                "--env",
                format!("RUNNER_SCOPE={}", config.github.runners.scope).as_str(),
                "--env",
                "EPHEMERAL=true",
                "--env",
                "UNSET_CONFIG_VARS=true",
                IMAGE,
            ],
        )?;
        info!(
            "[{}] Started a new container: {}",
            socket_addr, container_id
        );

        Ok(())
    }

    fn passphrase_opt(&self) -> Option<&str> {
        let passphrase = self.config.ssh.private_key_passphrase.as_str();
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
        command: &Vec<&str>,
    ) -> Result<String, Box<dyn Error>> {
        let env_script_path = Self::ssh_generate_env_script(socket_addr, session, env)?;

        // Prepend the command that sources the environment variable script and removes it.
        let mut cmd_with_env = vec![
            ".",
            env_script_path.as_str(),
            "&&",
            "rm",
            env_script_path.as_str(),
            "&&",
        ];
        for arg in command {
            cmd_with_env.push(arg);
        }

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
            &["mktemp", "-t", "self-hosted-runner-env.XXXXXXXXXX"],
        )?;

        let mut cmd = String::new();
        cmd.push_str("cat <<======== >");
        cmd.push_str_escaped(env_script_path.as_str());
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
        session: &mut Session,
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
        session: &mut Session,
        cmd: String,
    ) -> Result<String, Box<dyn Error>> {
        let mut ch = session.channel_session()?;
        ch.exec(cmd.as_str())?;

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
