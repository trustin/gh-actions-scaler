use crate::config::MachineConfig;
use log::{debug, info};
use ssh2::Session;
use std::error::Error;
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

    pub fn start_runner(&self) -> Result<(), Box<dyn Error>> {
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

        const IMAGE: &str = "ghcr.io/myoung34/docker-github-actions-runner:ubuntu-focal";

        info!(
            "[{}] Pulling the container image '{}' ..",
            socket_addr, IMAGE
        );
        self.ssh_exec(&socket_addr, &mut sess, format!("docker pull {}", IMAGE))?;

        info!("[{}] Pulled the container image", socket_addr);

        info!("[{}] Creating and starting a new container ..", socket_addr);
        let container_id = self.ssh_exec(
            &socket_addr,
            &mut sess,
            "docker run ".to_string() + "--detach " + "--label self-hosted-runner " + IMAGE,
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

    fn ssh_exec(
        &self,
        socket_addr: &SocketAddr,
        session: &mut Session,
        command: impl AsRef<str>,
    ) -> Result<String, Box<dyn Error>> {
        let mut ch = session.channel_session()?;
        let cmd = command.as_ref();
        ch.exec(cmd)?;

        let mut out = String::new();
        ch.read_to_string(&mut out)?;
        ch.wait_close()?;

        let out = out.trim();
        if ch.exit_status()? == 0 {
            Ok(out.to_string())
        } else {
            let mut indented_out: String = String::with_capacity(out.len() * 3 / 2);
            for line in out.lines() {
                indented_out.push_str("    ");
                indented_out.push_str(line);
                indented_out.push('\n');
            }

            Err(format!(
                "[{}] Failed to execute the command:\n\n    {}\n\nOutput:\n\n{}",
                socket_addr, cmd, indented_out
            )
            .into())
        }
    }
}
