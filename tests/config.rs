mod common;
mod config_test_fixture;

// TODO not impl feature
//  1. require field validation
//    1-1. github > runners > repo_url when scope=repo
//    1-2. machines > id
//    1-3. machines > ssh > host, port
//    1-4. machines > ssh > (username, password) or (private_key, public_key)
//  2. github > runners > default values

#[cfg(test)]
mod config_tests {
    use crate::config_test_fixture::{config_content_with_token, empty_config, one_machine_config};

    mod success {
        use crate::common::setup_file;
        use config::config::Config;

        #[test]
        fn all_empty_field() {
            // given
            let teardown_file =
                setup_file("success|all_empty_field", super::empty_config().as_str());

            // when
            let path = teardown_file.path();
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
            let teardown_file = setup_file(
                "success|one_machine",
                super::one_machine_config("machine-1", "172.18.0.100", 8022, "abc", "abc").as_str(),
            );

            // when
            let path = teardown_file.path();
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

            let machine = &machines[0]; // Using indexing instead of get
            assert_eq!(machine.id, Some("machine-1".to_string()));

            assert!(machine.runners.is_none());
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
            let teardown_file =
                setup_file("success|default_logger", super::empty_config().as_str());

            // when
            let path = teardown_file.path();
            let config_result = Config::try_from(path);

            // then
            assert!(
                config_result.is_ok(),
                "config file read fail. reason: `{:?}`",
                config_result.err()
            );
            assert_eq!(
                config_result.unwrap().log_level,
                Some(config::config::LogLevel::Info)
            );
        }
    }

    mod fail_yaml_parse {
        use crate::common::setup_file;
        use config::config::{Config, ConfigError};

        #[test]
        fn invalid_format() {
            // given
            let teardown_file = setup_file("fail_yaml_parse|invalid_format", "abc");

            // when
            let path = teardown_file.path();
            let config = Config::try_from(path);

            // then
            assert!(config.is_err());

            println!("{:?}", config);
            match config.err().unwrap() {
                ConfigError::ParseFailure { path: _, cause } => {}
                other_error => assert!(false, "is not ParseFailure. error: {:?}", other_error),
            }
        }

        #[test]
        fn no_github_access_token() {
            let empty_token_config: &str = "\
github:
  runners:
    name_prefix: acme
    scope: repo
    repo_url: https://github.com/foo/bar
machines:";
            // given
            let teardown_file =
                setup_file("fail_yaml_parse|no_github_access_token", empty_token_config);

            // when
            let path = teardown_file.path();
            let config = Config::try_from(path);

            // then
            assert!(config.is_err());

            println!("{:?}", config);
            match config.err().unwrap() {
                ConfigError::ParseFailure { path: _, cause } => {}
                other_error => assert!(false, "is not ParseFailure. error: {:?}", other_error),
            }
        }
    }

    mod file_read {
        use crate::common::setup_file;
        use config::config::{Config, ConfigError};
        use std::io::ErrorKind;
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        #[test]
        fn fail_permission_denied() {
            // given
            let teardown_file = setup_file(
                "file_read|fail_permission_denied",
                super::empty_config().as_str(),
            );
            let path = teardown_file.path();

            // TODO need?? window file permission test
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o200))
                .expect("test file permission change fail");

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
        use crate::common::setup_file;
        use config::config::{Config, ConfigError};

        #[test]
        fn success_brace() {
            // given
            std::env::set_var("VARIABLE_TEST1", "yes");

            let teardown_file = setup_file(
                "resolve_environment|success_brace",
                super::config_content_with_token("${VARIABLE_TEST1}").as_str(),
            );
            let path = teardown_file.path();

            // when
            let config = Config::try_from(path);

            // then
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
            let teardown_file = setup_file(
                "resolve_environment|fail_brace",
                super::config_content_with_token("${VARIABLE_TEST2}").as_str(),
            );
            let path = teardown_file.path();

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
        use crate::common::setup_file;
        use config::config::{Config, ConfigError};
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn success() {
            // given
            let teardown_token_file = setup_file("success_token", "1234567890");
            let teardown_config_file = setup_file(
                "resolve_file_variable|success",
                super::config_content_with_token("\"${file:success_token}\"").as_str(),
            );
            let path = teardown_config_file.path();

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
            let teardown_token_file = setup_file("permission_denied_token", "1234567890");
            std::fs::set_permissions(
                teardown_token_file.path(),
                std::fs::Permissions::from_mode(0o200),
            )
            .expect("test token file permission change fail");

            let teardown_config_file = setup_file(
                "resolve_file_variable|fail_permission_denied",
                super::config_content_with_token("\"${file:permission_denied_token}\"").as_str(),
            );
            let path = teardown_config_file.path();

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
            let teardown_file = setup_file(
                "resolve_file_variable|fail_not_exist",
                super::config_content_with_token("${file:not_exist_token}").as_str(),
            );
            let path = teardown_file.path();

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
