// base
struct TestConfig {
    log_level: Option<String>,
    github: Option<TestGithub>,
    machine_defaults: Option<TestMachineDefaults>,
    machines: Vec<TestMachine>,
}

impl TestConfig {
    fn to_yaml(&self) -> String {
        let log_level = self.log_level.as_deref().unwrap_or("");
        let github = match &self.github {
            None => "",
            Some(github) => &github.to_yaml(),
        };
        let machine_defaults = match &self.machine_defaults {
            None => "",
            Some(machine_defaults) => &machine_defaults.to_yaml(),
        };

        let machines: String = self
            .machines
            .iter()
            .map(|machine| machine.to_yaml())
            .collect::<Vec<String>>()
            .join("\n");

        format!(
            "log_level: {log_level}
github: {github}
machine_defaults: {machine_defaults}
machines: {machines}"
        )
    }
}

// github
struct TestGithub {
    personal_access_token: Option<String>,
    runners: Option<TestGithubRunners>,
}

impl TestGithub {
    fn to_yaml(&self) -> String {
        let personal_access_token = self.personal_access_token.as_deref().unwrap_or("");
        let runners = match &self.runners {
            None => "",
            Some(runners) => &runners.to_yaml(),
        };

        format!(
            "
  personal_access_token: {personal_access_token}
  runners: {runners}"
        )
    }
}

struct TestGithubRunners {
    name_prefix: Option<String>,
    scope: Option<String>,
    repo_url: Option<String>,
}

impl TestGithubRunners {
    fn to_yaml(&self) -> String {
        let name_prefix = self.name_prefix.as_deref().unwrap_or("");
        let scope = self.scope.as_deref().unwrap_or("");
        let repo_url = self.repo_url.as_deref().unwrap_or("");

        format!(
            "
    name_prefix: {name_prefix}
    scope: {scope}
    repo_url: {repo_url}"
        )
    }
}

// machine_default
struct TestMachineDefaults {
    ssh: Option<TestMachineDefaultsSsh>,
    runners: Option<TestMachineDefaultsRunners>,
}

impl TestMachineDefaults {
    fn to_yaml(&self) -> String {
        let ssh = match &self.ssh {
            None => "",
            Some(ssh) => &ssh.to_yaml(),
        };
        let runners = match &self.runners {
            None => "",
            Some(runners) => &runners.to_yaml(),
        };

        format!(
            "
  ssh: {ssh}
  runners: {runners}"
        )
    }
}

struct TestMachineDefaultsSsh {
    port: Option<String>,
    username: Option<String>,
}

impl TestMachineDefaultsSsh {
    fn to_yaml(&self) -> String {
        let port = self.port.as_deref().unwrap_or("");
        let username = self.username.as_deref().unwrap_or("");

        format!(
            "
    port: {port}
    username: {username}"
        )
    }
}

struct TestMachineDefaultsRunners {
    min_runners: Option<String>,
    max_runners: Option<String>,
    idle_timeout: Option<String>,
}

impl TestMachineDefaultsRunners {
    fn to_yaml(&self) -> String {
        let min_runners = self.min_runners.as_deref().unwrap_or("");
        let max_runners = self.max_runners.as_deref().unwrap_or("");
        let idle_timeout = self.idle_timeout.as_deref().unwrap_or("");

        format!(
            "
    min_runners: {min_runners}
    max_runners: {max_runners}
    idle_timeout: {idle_timeout}"
        )
    }
}

// machines
struct TestMachine {
    id: Option<String>,
    ssh: Option<TestMachineSsh>,
    runners: Option<TestMachineRunners>,
}

impl TestMachine {
    fn to_yaml(&self) -> String {
        let id = self.id.as_deref().unwrap_or("");
        let ssh = match &self.ssh {
            None => "",
            Some(ssh) => &ssh.to_yaml(),
        };
        let runners = match &self.runners {
            None => "",
            Some(runners) => &runners.to_yaml(),
        };

        format!(
            "
  - id: {id}
    ssh: {ssh}
    runners: {runners}"
        )
    }
}

struct TestMachineSsh {
    host: Option<String>,
    port: Option<String>,
    fingerprint: Option<String>,
    username: Option<String>,
    password: Option<String>,
    private_key: Option<String>,
    private_key_passphrase: Option<String>,
    public_key: Option<String>,
}

impl TestMachineSsh {
    fn to_yaml(&self) -> String {
        let host = self.host.as_deref().unwrap_or("");
        let port = self.port.as_deref().unwrap_or("");
        let fingerprint = self.fingerprint.as_deref().unwrap_or("");
        let username = self.username.as_deref().unwrap_or("");
        let password = self.password.as_deref().unwrap_or("");
        let private_key = self.private_key.as_deref().unwrap_or("");
        let private_key_passphrase = self.private_key_passphrase.as_deref().unwrap_or("");
        let public_key = self.public_key.as_deref().unwrap_or("");

        format!(
            "
      host: {host}
      port: {port}
      fingerprint: {fingerprint}
      username: {username}
      password: {password}
      private_key: {private_key}
      private_key_passphrase: {private_key_passphrase}
      public_key: {public_key}"
        )
    }
}

struct TestMachineRunners {
    min_runners: Option<String>,
    max_runners: Option<String>,
    idle_timeout: Option<String>,
}

impl TestMachineRunners {
    fn to_yaml(&self) -> String {
        let min_runners = self.min_runners.as_deref().unwrap_or("");
        let max_runners = self.max_runners.as_deref().unwrap_or("");
        let idle_timeout = self.idle_timeout.as_deref().unwrap_or("");

        format!(
            "
      min_runners: {min_runners}
      max_runners: {max_runners}
      idle_timeout: {idle_timeout}"
        )
    }
}

pub fn config_content_with_token(token: &str) -> String {
    let test_config: TestConfig = TestConfig {
        log_level: None,
        github: Some(TestGithub {
            personal_access_token: Some(token.to_string()),
            runners: None,
        }),
        machine_defaults: None,
        machines: vec![],
    };
    test_config.to_yaml()
}

pub fn one_machine_config(
    id: &str,
    host: &str,
    port: usize,
    username: &str,
    password: &str,
) -> String {
    let test_config: TestConfig = TestConfig {
        log_level: None,
        github: Some(TestGithub {
            personal_access_token: Some("1234567890".to_string()),
            runners: None,
        }),
        machine_defaults: None,
        machines: vec![TestMachine {
            id: Some(id.to_string()),
            ssh: Some(TestMachineSsh {
                host: Some(host.to_string()),
                port: Some(port.to_string()),
                username: Some(username.to_string()),
                password: Some(password.to_string()),
                fingerprint: None,
                private_key: None,
                private_key_passphrase: None,
                public_key: None,
            }),
            runners: None,
        }],
    };
    test_config.to_yaml()
}

pub fn empty_config() -> String {
    let test_config: TestConfig = TestConfig {
        log_level: None,
        github: Some(TestGithub {
            personal_access_token: Some("1234567890".to_string()),
            runners: None,
        }),
        machine_defaults: None,
        machines: vec![],
    };
    test_config.to_yaml()
}

#[cfg(test)]
mod config_test_fixture_test {
    use super::*;

    #[test]
    fn empty_config_to_yaml() {
        let test_config = TestConfig {
            log_level: None,
            github: None,
            machine_defaults: None,
            machines: vec![],
        };

        let yaml = test_config.to_yaml();

        assert_eq!(
            yaml,
            "log_level: \ngithub: \nmachine_defaults: \nmachines: "
        );
    }

    #[test]
    fn full_config_to_yaml() {
        let test_config = TestConfig {
            log_level: Some("info".to_string()),
            github: Some(TestGithub {
                personal_access_token: Some("personal_access_token".to_string()),
                runners: Some(TestGithubRunners {
                    name_prefix: Some("name_prefix".to_string()),
                    scope: Some("scope".to_string()),
                    repo_url: Some("repo_url".to_string()),
                }),
            }),
            machine_defaults: Some(TestMachineDefaults {
                ssh: Some(TestMachineDefaultsSsh {
                    port: Some("port".to_string()),
                    username: Some("username".to_string()),
                }),
                runners: Some(TestMachineDefaultsRunners {
                    min_runners: Some("min_runners".to_string()),
                    max_runners: Some("max_runners".to_string()),
                    idle_timeout: Some("idle_timeout".to_string()),
                }),
            }),
            machines: vec![TestMachine {
                id: Some("id".to_string()),
                ssh: Some(TestMachineSsh {
                    host: Some("host".to_string()),
                    port: Some("port".to_string()),
                    fingerprint: Some("fingerprint".to_string()),
                    username: Some("username".to_string()),
                    password: Some("password".to_string()),
                    private_key: Some("private_key".to_string()),
                    private_key_passphrase: Some("private_key_passphrase".to_string()),
                    public_key: Some("public_key".to_string()),
                }),
                runners: Some(TestMachineRunners {
                    min_runners: Some("min_runners".to_string()),
                    max_runners: Some("max_runners".to_string()),
                    idle_timeout: Some("idle_timeout".to_string()),
                }),
            }],
        };

        let yaml = test_config.to_yaml();

        assert_eq!(
            yaml,
            "log_level: info
github: \n  personal_access_token: personal_access_token
  runners: \n    name_prefix: name_prefix
    scope: scope
    repo_url: repo_url
machine_defaults: \n  ssh: \n    port: port
    username: username
  runners: \n    min_runners: min_runners
    max_runners: max_runners
    idle_timeout: idle_timeout
machines: \n  - id: id
    ssh: \n      host: host
      port: port
      fingerprint: fingerprint
      username: username
      password: password
      private_key: private_key
      private_key_passphrase: private_key_passphrase
      public_key: public_key
    runners: \n      min_runners: min_runners
      max_runners: max_runners
      idle_timeout: idle_timeout"
        );
    }
}
