mod common;

#[cfg(test)]
mod config_tests {

    fn config_content_with_token(token: &str) -> String {
        format!(
            "\
github:
  personal_access_token: {}
  runners:
    name_prefix: test
    scope: repo
    repo_url: https://github.com/foo/bar
machines:
  - id: machine-1
    ssh:
      host: 172.18.0.100
      username: JopopScript
      password: JopopScript",
            token
        )
    }

    static SIMPLE_CONFIG_CONTENT: &str = "\
github:
  personal_access_token: 1234567890
  runners:
    name_prefix: test
    scope: repo
    repo_url: https://github.com/foo/bar
machines:
  - id: machine-1
    ssh:
      host: 172.18.0.100
      username: JopopScript
      password: JopopScript";

    mod success {
        use crate::common::setup_file;
        use config::config::Config;

        #[test]
        fn simple() {
            // given
            let teardown_file = setup_file("success|simple", super::SIMPLE_CONFIG_CONTENT);

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
        fn all_empty_field() {
            // TODO impl
        }

        #[test]
        fn one_machine() {
            // TODO impl
        }

        #[test]
        fn two_machine() {
            // TODO impl
        }

        #[test]
        fn one_machine_and_default_machine() {
            // TODO impl
        }
    }

    mod yaml_parse {
        // TODO impl
        // ParseFailure { path: String, cause: serde_yaml_ng::Error }
    }

    mod file_read {
        use config::config::{Config, ConfigError};
        use std::io::ErrorKind;
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;
        use crate::common::setup_file;

        #[test]
        fn fail_permission_denied() {
            // given
            let teardown_file = setup_file(
                "file_read|fail_permission_denied",
                super::SIMPLE_CONFIG_CONTENT,
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
                    assert_eq!(cause.kind(), ErrorKind::PermissionDenied, "is not ReadFailure/PermissionDenied. error: {:?}", cause)
                }
                other_error => assert!(false, "is not ReadFailure/PermissionDenied. error: {:?}", other_error),
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
                    assert_eq!(cause.kind(), ErrorKind::NotFound, "is not ReadFailure/NotFound. error: {:?}", cause)
                }
                other_error => assert!(false, "is not ReadFailure/NotFound. error: {:?}", other_error),
            }
        }
    }

    mod resolve_environment {
        use config::config::{Config, ConfigError};
        use crate::common::setup_file;

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
            assert!(config.is_ok(), "is not ok. return error: {:?}", config.err());
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
                other_error => assert!(false, "is not UnresolvedEnvironmentVariable. error: {:?}", other_error),
            }
        }
    }

    mod resolve_file_variable {
        use config::config::{Config, ConfigError};
        use std::os::unix::fs::PermissionsExt;
        use crate::common::setup_file;

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
            std::fs::set_permissions(teardown_token_file.path(), std::fs::Permissions::from_mode(0o200))
                .expect("test token file permission change fail");

            let teardown_config_file = setup_file(
                "resolve_file_variable|success_config",
                super::config_content_with_token("\"${file:permission_denied_token}\"").as_str(),
            );
            let path = teardown_config_file.path();

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err(), "config parse not fail. expect fail");
            match config.err().unwrap() {
                ConfigError::UnresolvedFileVariable { .. } => {}
                other_error => assert!(false, "is not UnresolvedFileVariable. error: {:?}", other_error),
            }
        }

        #[test]
        fn fail_not_exist() {
            // given
            let teardown_file = setup_file(
                "resolve_file_variable|success_config",
                super::config_content_with_token("${file:not_exist_token}").as_str(),
            );
            let path = teardown_file.path();

            // when
            let config = Config::try_from(path);

            // then
            assert!(config.is_err(), "config parse not fail. expect fail");
            match config.err().unwrap() {
                ConfigError::UnresolvedFileVariable { .. } => {},
                other_error => assert!(false, "is not UnresolvedFileVariable. error: {:?}", other_error),
            }
        }
    }
}
