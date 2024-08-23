mod common;

#[cfg(test)]
mod config_tests {
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
        use crate::common;
        use gh_actions_scaler::config::Config;

        #[test]
        fn simple() {
            // given
            let path_buf =
                common::setup_create_file("success|simple", super::SIMPLE_CONFIG_CONTENT)
                    .expect("test config file create error");
            let path = path_buf.as_path();

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new(path);
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
        use crate::common;
        use gh_actions_scaler::config::{Config, ConfigError};
        use std::io::ErrorKind;
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        #[test]
        fn fail_permission_denied() {
            // given
            let path_buf = common::setup_create_file(
                "file_read|fail_permission_denied",
                super::SIMPLE_CONFIG_CONTENT,
            )
            .expect("test config file create error");
            let path = path_buf.as_path();

            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o200))
                .expect("test file permission change fail");

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new(path);
            match config.err().unwrap() {
                ConfigError::ReadFailure { path: _, cause } => {
                    assert!(
                        cause.kind() == ErrorKind::PermissionDenied,
                        "errorKinkd is not PermissionDenied"
                    )
                }
                _ => assert!(false, "ConfigError is not ReadFailure"),
            }
        }

        #[test]
        fn fail_not_exist() {
            // given
            let path = Path::new("file_read|fail_not_exist");

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new(path);
            match config.err().unwrap() {
                ConfigError::ReadFailure { path: _, cause } => {
                    assert!(
                        cause.kind() == ErrorKind::NotFound,
                        "errorKinkd is not NotFound"
                    )
                }
                _ => assert!(false, "ConfigError is not ReadFailure"),
            }
        }
    }

    mod resolve_environment {
        use crate::common;
        use gh_actions_scaler::config::{Config, ConfigError};

        #[test]
        fn success_brace() {
            // given
            std::env::set_var("VARIABLE_TEST1", "yes");

            let content = super::config_content_with_token("${VARIABLE_TEST1}");
            let path_buf =
                common::setup_create_file("resolve_environment|success_brace", content.as_str())
                    .expect("test config file create error");
            let path = path_buf.as_path();

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new(path);
            assert!(
                config.is_ok(),
                "config parse fail error: {:?}",
                config.err()
            );
            assert_eq!(config.unwrap().github.personal_access_token, "yes");
        }

        #[test]
        fn fail_brace() {
            // given
            let content = super::config_content_with_token("${VARIABLE_TEST2}");
            let path_buf =
                common::setup_create_file("resolve_environment|fail_brace", content.as_str())
                    .expect("test config file create error");
            let path = path_buf.as_path();

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new(path);
            assert!(config.is_err(), "config parse not fail. expect fail...");
            match config.err().unwrap() {
                ConfigError::UnresolvedEnvironmentVariable { .. } => {}
                _ => assert!(false, "ConfigError is not UnresolvedEnvironmentVariable"),
            }
        }
    }

    mod resolve_file_variable {
        use crate::common;
        use gh_actions_scaler::config::{Config, ConfigError};
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn success() {
            // given
            let content = super::config_content_with_token("\"${file:test_token}\"");

            let token_path_buf = common::setup_create_file("test_token", "1234567890")
                .expect("test token file create error");
            let token_path = token_path_buf.as_path();

            let path_buf =
                common::setup_create_file("resolve_file_variable|success", content.as_str())
                    .expect("test config file create error");
            let path = path_buf.as_path();

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new_vec(vec![path, token_path]);
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
            let content = super::config_content_with_token("${file:permission_denied_token}");

            let token_path_buf = common::setup_create_file("permission_denied_token", "1234567890")
                .expect("test token file create error");
            let token_path = token_path_buf.as_path();
            std::fs::set_permissions(token_path, std::fs::Permissions::from_mode(0o200))
                .expect("test token file permission change fail");

            let path_buf = common::setup_create_file(
                "resolve_file_variable|fail_permission_denied",
                content.as_str(),
            )
            .expect("test config file create error");
            let path = path_buf.as_path();

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new_vec(vec![token_path, path]);
            assert!(config.is_err(), "config parse not fail. expect fail...");
            match config.err().unwrap() {
                ConfigError::UnresolvedFileVariable { .. } => {}
                _ => assert!(false, "ConfigError is not UnresolvedEnvironmentVariable"),
            }
        }

        #[test]
        fn fail_not_exist() {
            // given
            let content = super::config_content_with_token("${file:permission_denied_token}");

            let path_buf =
                common::setup_create_file("resolve_file_variable|fail_not_exist", content.as_str())
                    .expect("test config file create error");
            let path = path_buf.as_path();

            // when
            let config = Config::try_from(path);

            // then
            common::TeardownFile::new(path);
            assert!(config.is_err(), "config parse not fail. expect fail...");
            match config.err().unwrap() {
                ConfigError::UnresolvedFileVariable { .. } => {}
                _ => assert!(false, "ConfigError is not UnresolvedEnvironmentVariable"),
            }
        }
    }

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
}
