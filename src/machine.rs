use crate::config::{Config, MachineConfig};
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

    pub fn start_runner(&self, config: &Config, run_url: &str) -> Result<(), Box<dyn Error>> {
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

        // TODO inject cache_file_path ~/.cache/gh-actions-scaler (or $XDG_CACHE_HOME/...)
        let cache_dir = "~/.cache";
        let cache_file_name = "gh-actions-scaler";
        let is_valid_cache_image =
            Self::is_valid_cache_image(cache_dir, cache_file_name, &socket_addr, &mut sess)
                .unwrap_or_else(|err| {
                    // FIXME cant get current time or cant use cache version -> always image pulling
                    //       for example cache file permission denied.
                    warn!(
                "[{}] Failed to open cache file or get current time. file_path: '{}/{}'. err: {}",
                socket_addr, cache_dir, cache_file_name, err
            );
                    false
                });

        if !is_valid_cache_image {
            info!(
                "[{}] Pulling the container image '{}' ..",
                socket_addr, IMAGE
            );
            Self::ssh_exec(&socket_addr, &mut sess, &["docker", "image", "pull", IMAGE])?;
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
        let container_id = Self::ssh_exec_with_env(
            &socket_addr,
            &mut sess,
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
            socket_addr, container_id
        );

        Ok(())
    }

    /// Returns cache container image is valid
    /// # Returns
    ///
    /// * `Ok(true)` - Container image valid and has not expired
    /// * `Ok(false)` - Container image has expired. is no longer valid
    /// * `Error` - An I/O error occurred while attempting to read(or write) the cache information file.
    fn is_valid_cache_image(
        dir: &str,
        file_name: &str,
        socket_addr: &SocketAddr,
        sess: &mut Session,
    ) -> Result<bool, Box<dyn Error>> {
        let now_version = Self::now_cache_version()?;

        let mut pull_path = String::new();
        pull_path.push_str(dir);
        pull_path.push('/');
        pull_path.push_str(file_name);

        let is_exist_file = Self::ssh_exec(socket_addr, sess, &["test", "-f", &pull_path])
            .map(|_| true)
            .unwrap_or(false);

        if !is_exist_file {
            Self::ssh_exec(socket_addr, sess, &["mkdir", "-p", dir])?;
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

        //TODO need convert datetime(yyhhdd) ex) "241125"
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
