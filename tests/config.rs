#[macro_use(defer)]
extern crate scopeguard;

// TODO not impl feature
//  1. require field validation
//    1-5. machines > ssh > host, port
//    1-6. machines > ssh > (username, password) or (private_key, public_key)
//    1-7. machines > runners > min, max, idle_timeout

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
        fn minimal() {
            read_config("tests/fixtures/config/minimal.yaml");
        }

        #[test]
        fn one_machine() {
            let config = read_config("tests/fixtures/config/one_machine.yaml");
            let machines = config.machines;
            assert_that!(machines).has_length(1);
            assert_that!(machines).contains(MachineConfig {
                id: "machine-alpha".to_string(),
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
            let config = read_config("tests/fixtures/config/minimal.yaml");
            assert_that!(config.log_level).is_equal_to(LogLevel::Info);
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
        use gh_actions_scaler::config::ConfigError;
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
