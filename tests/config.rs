#[macro_use(defer)]
extern crate scopeguard;

#[cfg(test)]
mod config_tests {
    use gh_actions_scaler::config::{Config, ConfigError};
    use speculoos::prelude::*;
    use std::path::Path;

    mod success {
        use crate::config_tests::read_config;
        use gh_actions_scaler::config::{
            Config, GithubConfig, GithubRunnerConfig, LogLevel, MachineConfig,
            MachineDefaultsConfig, RunnersConfig, SshConfig,
        };
        use speculoos::prelude::*;

        #[test]
        fn minimal() {
            let config = read_config("tests/fixtures/config/minimal.yaml");

            assert_that!(config).is_equal_to(Config {
                log_level: LogLevel::Info,
                github: GithubConfig {
                    personal_access_token: "ghp_my_secret_token".to_string(),
                    runners: GithubRunnerConfig {
                        name_prefix: "runner".to_string(),
                        scope: "repo".to_string(),
                        repo_url: "https://github.com/trustin/gh-actions-scaler".to_string(),
                        // TODO(trustin): Write a test case for GHE URLs.
                        api_endpoint_url: "https://api.github.com".to_string(),
                        repo_user: "trustin".to_string(),
                        repo_name: "gh-actions-scaler".to_string(),
                    },
                },
                machine_defaults: MachineDefaultsConfig {
                    ssh: SshConfig {
                        host: "".to_string(),
                        port: 0,
                        fingerprint: "".to_string(),
                        username: "".to_string(),
                        password: "".to_string(),
                        private_key: "".to_string(),
                        private_key_passphrase: "".to_string(),
                    },
                    runners: RunnersConfig { max: 0 },
                },
                machines: vec![MachineConfig {
                    id: "machine-1".to_string(),
                    runners: RunnersConfig { max: 16 },
                    ssh: SshConfig {
                        host: "alpha.example.tld".to_string(),
                        port: 22,
                        fingerprint: "".to_string(),
                        username: "trustin".to_string(),
                        password: "my_secret_password".to_string(),
                        private_key: "".to_string(),
                        private_key_passphrase: "".to_string(),
                    },
                }],
            });
        }

        #[test]
        fn default_log_level() {
            let config = read_config("tests/fixtures/config/minimal.yaml");
            assert_that!(config.log_level).is_equal_to(LogLevel::Info);
        }

        #[test]
        fn default_runners_config() {
            let config = read_config("tests/fixtures/config/default_runners_config.yaml");
            let machines = &config.machines;
            assert_that!(machines).has_length(1);
            assert_that!(machines[0].runners).is_equal_to(RunnersConfig { max: 16 });
        }
    }

    mod parse_failure {
        use crate::config_tests::read_invalid_config;
        use gh_actions_scaler::config::ConfigError;

        #[test]
        fn parse_failure() {
            assert!(matches!(
                read_invalid_config("tests/fixtures/config/invalid_format.yaml"),
                ConfigError::ParseFailure { .. }
            ));
        }
    }

    mod read_failure {
        use crate::config_tests::read_invalid_config;
        use gh_actions_scaler::config::ConfigError;
        use speculoos::assert_that;
        use std::io::ErrorKind;

        #[test]
        fn non_existent_file() {
            let path = "non_existent_file.yaml";
            let err = read_invalid_config(path);
            match err {
                ConfigError::ReadFailure {
                    path: actual_path,
                    cause,
                } => {
                    assert_that!(actual_path.as_str()).is_equal_to(path);
                    assert_that!(cause.kind()).is_equal_to(ErrorKind::NotFound);
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ReadFailure)", err);
                }
            }
        }
    }

    mod env_var_substitution {
        use crate::config_tests::{read_config, read_invalid_config};
        use gh_actions_scaler::config::ConfigError;
        use serial_test::serial;
        use speculoos::prelude::*;
        use std::env::VarError;

        #[test]
        #[serial(env_var)]
        fn success() {
            std::env::set_var("GH_ACTIONS_SCALER_FOO", "ghp_my_secret_token");
            defer! {
                std::env::remove_var("GH_ACTIONS_SCALER_FOO");
            }

            let config = read_config("tests/fixtures/config/env_var_substitution.yaml");
            assert_that!(config.github.personal_access_token.as_str())
                .is_equal_to("ghp_my_secret_token");
        }

        #[test]
        #[serial(env_var)]
        fn missing_env_var() {
            let err = read_invalid_config("tests/fixtures/config/env_var_substitution.yaml");
            match err {
                ConfigError::UnresolvedEnvironmentVariable { name, cause } => {
                    assert_that!(name.as_ref()).is_equal_to("GH_ACTIONS_SCALER_FOO");
                    assert!(matches!(cause, VarError::NotPresent));
                }
                _ => {
                    panic!(
                        "Unexpected: {:?} (expected: UnresolvedEnvironmentVariable)",
                        err
                    );
                }
            }
        }
    }

    mod file_substitution {
        use crate::config_tests::{read_config, read_invalid_config};
        use gh_actions_scaler::config::ConfigError;
        use speculoos::prelude::*;
        use std::io::ErrorKind;

        #[test]
        fn success() {
            let config = read_config("tests/fixtures/config/file_substitution_success.yaml");
            assert_that!(config.github.personal_access_token.as_str())
                .is_equal_to("ghp_my_secret_token");
        }

        #[test]
        fn non_existent_file() {
            let err = read_invalid_config(
                "tests/fixtures/config/file_substitution_non_existent_file.yaml",
            );
            match err {
                ConfigError::UnresolvedFileVariable { path, cause } => {
                    assert_that!(path.as_str())
                        .is_equal_to("tests/fixtures/config/non_existent_file");
                    assert_that!(cause.kind()).is_equal_to(ErrorKind::NotFound);
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: UnresolvedFileVariable)", err);
                }
            }
        }
    }

    mod github {
        use crate::config_tests::read_invalid_config;
        use gh_actions_scaler::config::ConfigError;
        use speculoos::prelude::*;

        #[test]
        fn empty_or_missing_personal_access_token() {
            let err = read_invalid_config(
                "tests/fixtures/config/empty_or_missing_personal_access_token.yaml",
            );
            match err {
                ConfigError::ValidationFailure { message } => {
                    assert_that!(message.as_str()).contains("github.personal_access_token");
                    assert_that!(message.as_str()).contains("empty or missing");
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ValidationFailure)", err);
                }
            }
        }

        #[test]
        fn invalid_personal_access_token() {
            let err =
                read_invalid_config("tests/fixtures/config/invalid_personal_access_token.yaml");
            match err {
                ConfigError::ValidationFailure { message } => {
                    assert_that!(message.as_str()).contains("github.personal_access_token");
                    assert_that!(message.as_str()).contains("invalid");
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ValidationFailure)", err);
                }
            }
        }

        #[test]
        fn empty_name_prefix() {
            let err = read_invalid_config("tests/fixtures/config/empty_name_prefix.yaml");
            match err {
                ConfigError::ValidationFailure { message } => {
                    assert_that!(message.as_str()).contains("github.runners.name_prefix");
                    assert_that!(message.as_str()).contains("empty");
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ValidationFailure)", err);
                }
            }
        }

        #[test]
        fn empty_or_missing_repo_url() {
            let err = read_invalid_config("tests/fixtures/config/empty_or_missing_repo_url.yaml");
            match err {
                ConfigError::ValidationFailure { message } => {
                    assert_that!(message.as_str()).contains("github.runners.repo_url");
                    assert_that!(message.as_str()).contains("empty or missing");
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ValidationFailure)", err);
                }
            }
        }

        #[test]
        fn invalid_repo_url() {
            let err = read_invalid_config("tests/fixtures/config/invalid_repo_url.yaml");
            match err {
                ConfigError::ValidationFailure { message } => {
                    assert_that!(message.as_str()).contains("github.runners.repo_url");
                    assert_that!(message.as_str()).contains("invalid");
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ValidationFailure)", err);
                }
            }
        }
    }

    mod machines {
        use crate::config_tests::read_config;
        use crate::config_tests::read_invalid_config;
        use gh_actions_scaler::config::{ConfigError, MachineConfig, RunnersConfig, SshConfig};
        use speculoos::prelude::*;

        #[test]
        fn empty_machines() {
            let err = read_invalid_config("tests/fixtures/config/empty_machines.yaml");
            match err {
                ConfigError::ValidationFailure { message } => {
                    assert_that!(message.as_str()).contains("at least one machine");
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ValidationFailure)", err);
                }
            }
        }
        #[test]
        fn duplicate_machine_id() {
            let err = read_invalid_config("tests/fixtures/config/duplicate_machine_id.yaml");
            match err {
                ConfigError::ValidationFailure { message } => {
                    assert_that!(message.as_str()).contains("duplicate machine ID");
                    assert_that!(message.as_str()).contains("'machine-alpha'");
                }
                _ => {
                    panic!("Unexpected: {:?} (expected: ValidationFailure)", err);
                }
            }
        }

        #[test]
        fn generated_machine_id() {
            let config = read_config("tests/fixtures/config/generated_machine_id.yaml");
            let machines = &config.machines;
            assert_that!(machines).has_length(4);
            assert_that!(machines[0].id.as_str()).is_equal_to("machine-1");
            assert_that!(machines[1].id.as_str()).is_equal_to("machine-2");
            assert_that!(machines[2].id.as_str()).is_equal_to("machine-3");
            assert_that!(machines[3].id.as_str()).is_equal_to("machine-4");
        }

        #[test]
        fn machines_without_defaults() {
            let config = read_config("tests/fixtures/config/machines_without_defaults.yaml");
            let machines = config.machines;
            assert_that!(machines).is_equal_to(vec![
                MachineConfig {
                    id: "machine-alpha".to_string(),
                    ssh: SshConfig {
                        host: "172.18.0.100".to_string(),
                        port: 8022,
                        fingerprint: "12:34:56:78:9a:bc:de:f0:11:22:33:44:55:66:77:88".to_string(),
                        username: "abc".to_string(),
                        password: "def".to_string(),
                        private_key: "".to_string(),
                        // Must be ignored because using password auth
                        private_key_passphrase: "".to_string(),
                    },
                    runners: RunnersConfig { max: 3 },
                },
                MachineConfig {
                    id: "machine-beta".to_string(),
                    ssh: SshConfig {
                        host: "172.18.0.101".to_string(),
                        port: 22,
                        fingerprint: "".to_string(),
                        username: "ghi".to_string(),
                        password: "".to_string(),
                        private_key: "jkl".to_string(),
                        private_key_passphrase: "mno".to_string(),
                    },
                    runners: RunnersConfig { max: 16 },
                },
                MachineConfig {
                    id: "machine-theta".to_string(),
                    ssh: SshConfig {
                        host: "172.18.0.102".to_string(),
                        port: 22,
                        fingerprint: "".to_string(),
                        username: "pqr".to_string(),
                        // Must be ignored because using private key auth
                        password: "".to_string(),
                        private_key: "stu".to_string(),
                        private_key_passphrase: "vwx".to_string(),
                    },
                    runners: RunnersConfig { max: 16 },
                },
            ]);
        }

        #[test]
        fn machines_with_defaults() {
            let config = read_config("tests/fixtures/config/machines_with_defaults.yaml");
            let machines = config.machines;
            assert_that!(machines).is_equal_to(vec![
                MachineConfig {
                    id: "machine-alpha".to_string(),
                    ssh: SshConfig {
                        host: "default_host".to_string(),
                        port: 8022,
                        fingerprint: "".to_string(),
                        username: "default_username".to_string(),
                        // The default password must be ignored,
                        // because the default private key was specified *and* no per-machine auth was configured.
                        password: "".to_string(),
                        private_key: "default_private_key".to_string(),
                        private_key_passphrase: "default_private_key_passphrase".to_string(),
                    },
                    runners: RunnersConfig { max: 16 },
                },
                MachineConfig {
                    id: "machine-beta".to_string(),
                    ssh: SshConfig {
                        host: "172.18.0.101".to_string(),
                        port: 10022,
                        fingerprint: "12:34:56:78:9a:bc:de:f0:11:22:33:44:55:66:77:88".to_string(),
                        username: "abc".to_string(),
                        password: "def".to_string(),
                        // The default private key must be ignored,
                        // because the per-machine password was specified.
                        private_key: "".to_string(),
                        private_key_passphrase: "".to_string(),
                    },
                    runners: RunnersConfig { max: 16 },
                },
                MachineConfig {
                    id: "machine-theta".to_string(),
                    ssh: SshConfig {
                        host: "172.18.0.102".to_string(),
                        port: 8022,
                        fingerprint: "".to_string(),
                        username: "default_username".to_string(),
                        // The default password must be ignored,
                        // because the per-machine private key was specified.
                        password: "".to_string(),
                        private_key: "ghi".to_string(),
                        private_key_passphrase: "jkl".to_string(),
                    },
                    runners: RunnersConfig { max: 16 },
                },
            ]);
        }

        #[test]
        fn default_machine_runners_config() {
            let config = read_config("tests/fixtures/config/default_machine_runners_config.yaml");
            let machines = &config.machines[0];
            assert_that!(machines.runners.max).is_equal_to(16);
        }

        #[test]
        fn default_machine_runners_config_from_defaults() {
            let config = read_config(
                "tests/fixtures/config/default_machine_runners_config_from_defaults.yaml",
            );
            let machines = &config.machines[0];
            assert_that!(machines.runners.max).is_equal_to(8);
        }

        #[test]
        fn overridden_machine_runners_config() {
            let config =
                read_config("tests/fixtures/config/overridden_machine_runners_config.yaml");
            let machines = &config.machines[0];
            assert_that!(machines.runners.max).is_equal_to(4);
        }
    }

    fn read_config<P: AsRef<Path> + ?Sized>(path: &P) -> Config {
        let file = path.as_ref();
        let result = Config::try_from(file);
        assert_that!(result).is_ok();
        result.unwrap()
    }

    fn read_invalid_config<P: AsRef<Path> + ?Sized>(path: &P) -> ConfigError {
        let file = path.as_ref();
        let result = Config::try_from(file);
        assert_that!(result).is_err();
        result.unwrap_err()
    }
}
