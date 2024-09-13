mod common;
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
    mod success {
        use gh_actions_scaler::config::{Config, LogLevel};
        use std::path::Path;

        #[test]
        fn all_empty_field() {
            // given
            let path = Path::new("tests/fixtures/config/empty.yaml");

            // when
            let config = Config::try_from(path);

            // then
            assert!(
                config.is_ok(),
                "config file read fail. reason: `{:?}`",
                config.err()
            );
        }

        #[test]
        fn one_machine() {
            // given
            let path = Path::new("tests/fixtures/config/one_machine.yaml");

            // when
            let config = Config::try_from(path);

            // then
            assert!(
                config.is_ok(),
                "config file read fail. reason: `{:?}`",
                config.err()
            );
            let config = config.unwrap();

            let machines = &config.machines;
            assert_eq!(machines.len(), 1);

            let machine = &machines[0];
            assert_eq!(machine.id, Some("machine-1".to_string()));

            assert!(machine.runners.is_some());
            let runners = machine.runners.as_ref().unwrap();
            assert_eq!(runners.min, Some(2));
            assert_eq!(runners.max, Some(2));
            assert_eq!(runners.idle_timeout, Some("5m".to_string()));

            assert!(machine.ssh.is_some());
            let ssh = machine.ssh.as_ref().unwrap();

            assert_eq!(ssh.host, Some("172.18.0.100".to_string()));
            assert_eq!(ssh.port, Some(8022));
            assert_eq!(ssh.fingerprint, None);
            assert_eq!(ssh.username, Some("abc".to_string()));
            assert_eq!(ssh.password, Some("abc".to_string()));
            assert_eq!(ssh.private_key, None);
            assert_eq!(ssh.private_key_passphrase, None);
        }

        #[test]
        fn default_logger() {
            // given
            let path = Path::new("tests/fixtures/config/empty.yaml");

            // when
            let config_result = Config::try_from(path);

            // then
            assert!(
                config_result.is_ok(),
                "config file read fail. reason: `{:?}`",
                config_result.err()
            );
            assert_eq!(config_result.unwrap().log_level, Some(LogLevel::Info));
        }

        #[test]
        fn default_runners() {
            // given
            let path = Path::new("tests/fixtures/config/default_runner_machine.yaml");

            // when
            let config = Config::try_from(path);

            // then
            assert!(
                config.is_ok(),
                "config file read fail. reason: `{:?}`",
                config.err()
            );
            let config = config.unwrap();

            let machines = &config.machines;
            assert_eq!(machines.len(), 1);

            let machine = &machines[0];

            assert!(machine.runners.is_some());
            let runners = machine.runners.as_ref().unwrap();
            assert_eq!(runners.min, Some(1));
            assert_eq!(runners.max, Some(16));
            assert_eq!(runners.idle_timeout, Some("1m".to_string()));
        }
    }

    mod fail_yaml_parse {
        use gh_actions_scaler::config::{Config, ConfigError};
        use std::path::Path;

        #[test]
        fn invalid_format() {
            // given
            let path = Path::new("tests/fixtures/config/invalid_format.yaml");

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err());

            println!("{:?}", config);
            match config.err().unwrap() {
                ConfigError::ParseFailure { .. } => {}
                other_error => assert!(false, "is not ParseFailure. error: {:?}", other_error),
            }
        }
    }

    mod file_read {
        use crate::common::TeardownPermissionDenied;
        use gh_actions_scaler::config::{Config, ConfigError};
        use std::io::ErrorKind;
        use std::path::Path;

        // TODO need?? window file permission test -> i dont have window...
        #[test]
        fn fail_permission_denied() {
            // given
            let path = Path::new("tests/fixtures/config/permission_denied.yaml");
            let teardown = TeardownPermissionDenied::from(path);

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err());
            match config.err().unwrap() {
                ConfigError::ReadFailure { path: _, cause } => {
                    assert_eq!(
                        cause.kind(),
                        ErrorKind::PermissionDenied,
                        "is not ReadFailure/PermissionDenied. error: {:?}",
                        cause
                    )
                }
                other_error => assert!(
                    false,
                    "is not ReadFailure/PermissionDenied. error: {:?}",
                    other_error
                ),
            }
        }

        #[test]
        fn fail_not_exist() {
            // given
            let path = Path::new("file_read|fail_not_exist");

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err());
            match config.err().unwrap() {
                ConfigError::ReadFailure { path: _, cause } => {
                    assert_eq!(
                        cause.kind(),
                        ErrorKind::NotFound,
                        "is not ReadFailure/NotFound. error: {:?}",
                        cause
                    )
                }
                other_error => assert!(
                    false,
                    "is not ReadFailure/NotFound. error: {:?}",
                    other_error
                ),
            }
        }
    }

    mod resolve_environment {
        use gh_actions_scaler::config::{Config, ConfigError};
        use std::path::Path;

        #[test]
        fn success_brace() {
            // given
            std::env::set_var("CONFIG_VARIABLE_TEST", "yes");

            let path = Path::new("tests/fixtures/config/with_brace_token.yaml");

            // when
            let config = Config::try_from(path);

            // then
            std::env::remove_var("CONFIG_VARIABLE_TEST");
            assert!(
                config.is_ok(),
                "is not ok. return error: {:?}",
                config.err()
            );
            assert_eq!(config.unwrap().github.personal_access_token, "yes");
        }

        #[test]
        fn fail_brace() {
            // given
            let path = Path::new("tests/fixtures/config/with_not_exist_brace_token.yaml");

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err(), "config parse not fail. expect fail");
            match config.err().unwrap() {
                ConfigError::UnresolvedEnvironmentVariable { .. } => {}
                other_error => assert!(
                    false,
                    "is not UnresolvedEnvironmentVariable. error: {:?}",
                    other_error
                ),
            }
        }
    }

    mod resolve_file_variable {
        use crate::common::TeardownPermissionDenied;
        use gh_actions_scaler::config::{Config, ConfigError};
        use std::path::Path;

        #[test]
        fn success() {
            // given
            let path = Path::new("tests/fixtures/config/file_token_resolve.yaml");

            // when
            let config = Config::try_from(path);

            // then
            assert!(
                config.is_ok(),
                "config parse fail error: {:?}",
                config.err()
            );
            assert_eq!(config.unwrap().github.personal_access_token, "1234567890");
        }

        #[test]
        fn fail_permission_denied() {
            // given
            let path = Path::new("tests/fixtures/config/file_token_permission_denied.yaml");
            let teardown = TeardownPermissionDenied::from(Path::new(
                "tests/fixtures/config/permission_denied_token",
            ));

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err(), "config parse not fail. expect fail");
            match config.err().unwrap() {
                ConfigError::UnresolvedFileVariable { .. } => {}
                other_error => assert!(
                    false,
                    "is not UnresolvedFileVariable. error: {:?}",
                    other_error
                ),
            }
        }

        #[test]
        fn fail_not_exist() {
            // given
            let path = Path::new("tests/fixtures/config/file_token_not_exist.yaml");

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err(), "config parse not fail. expect fail");
            match config.err().unwrap() {
                ConfigError::UnresolvedFileVariable { .. } => {}
                other_error => assert!(
                    false,
                    "is not UnresolvedFileVariable. error: {:?}",
                    other_error
                ),
            }
        }
    }
}
