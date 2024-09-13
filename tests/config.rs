#[macro_use(defer)]
extern crate scopeguard;

// TODO not impl feature
//  1. require field validation
//    1-1. github > personal_access_token
//    1-2. github > runners > repo_url when scope=repo
//    1-3. machines empty -> At least one machine is required
//    1-4. machines > id
//    1-5. machines > ssh > host, port
//    1-6. machines > ssh > (username, password) or (private_key, public_key)
//  2. github > runners > default values

#[cfg(test)]
mod config_tests {
    use gh_actions_scaler::config::{Config, ConfigError};
    use speculoos::prelude::*;
    use std::path::Path;

    mod success {
        use crate::config_tests::read_config;
        use gh_actions_scaler::config::{LogLevel, MachineConfig, RunnersConfig, SshConfig};
        use speculoos::prelude::*;

        #[test]
        fn all_empty_field() {
            read_config("tests/fixtures/config/empty.yaml");
        }

        #[test]
        fn one_machine() {
            let config = read_config("tests/fixtures/config/one_machine.yaml");
            let machines = config.machines;
            assert_that!(machines).has_length(1);
            assert_that!(machines).contains(MachineConfig {
                id: Some("machine-1".to_string()),
                runners: Some(RunnersConfig {
                    min: Some(2),
                    max: Some(3),
                    idle_timeout: Some("5m".to_string()),
                }),
                ssh: Some(SshConfig {
                    host: Some("172.18.0.100".to_string()),
                    port: Some(8022),
                    fingerprint: None,
                    username: Some("abc".to_string()),
                    password: Some("def".to_string()),
                    private_key: None,
                    private_key_passphrase: None,
                }),
            });
        }

        #[test]
        fn default_log_level() {
            let config = read_config("tests/fixtures/config/empty.yaml");
            assert_that!(config.log_level).contains(LogLevel::Info);
        }

        #[test]
        fn default_runners_config() {
            let config = read_config("tests/fixtures/config/default_runners_config.yaml");
            let machines = &config.machines;
            assert_that!(machines).has_length(1);
            assert_that!(machines[0].runners).is_equal_to(Some(RunnersConfig {
                min: Some(1),
                max: Some(16),
                idle_timeout: Some("1m".to_string()),
            }));
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
            std::env::set_var("GH_ACTIONS_SCALER_FOO", "my_secret_token");
            defer! {
                std::env::remove_var("GH_ACTIONS_SCALER_FOO");
            }

            let config = read_config("tests/fixtures/config/with_brace_token.yaml");
            assert_that!(config.github.personal_access_token.as_str())
                .is_equal_to("my_secret_token");
        }

        #[test]
        #[serial(env_var)]
        fn missing_env_var() {
            let err = read_invalid_config("tests/fixtures/config/with_brace_token.yaml");
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
            let config = read_config("tests/fixtures/config/file_token_resolve.yaml");
            assert_that!(config.github.personal_access_token.as_str()).is_equal_to("1234567890");
        }

        #[test]
        fn non_existent_file() {
            let err = read_invalid_config("tests/fixtures/config/file_token_not_exist.yaml");
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
