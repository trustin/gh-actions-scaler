use std::cell::RefCell;
use std::{env, fs};
use std::path::{Path, PathBuf};
use once_cell::sync::Lazy;
use regex::{Captures, Regex, Replacer};
use crate::config::ConfigError;

pub struct ConfigResolver {
    config_dir: PathBuf,
}

impl <P: AsRef<Path>> From<P> for ConfigResolver {
    fn from(config_dir: P) -> Self {
        ConfigResolver { config_dir: PathBuf::from(config_dir.as_ref()) }
    }
}

impl ConfigResolver {
    pub fn resolve_opt<STR: AsRef<str>>(&self, input: &Option<STR>) -> Result<Option<String>, ConfigError> {
        match input {
            Some(value) => Ok(Some(self.resolve(value)?)),
            None => Ok(None),
        }
    }

    pub fn resolve<STR: AsRef<str>>(&self, input: STR) -> Result<String, ConfigError> {
        static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\$\$)|\$\{(file:)?([^}]+)}").unwrap());
        let config_error_ref: RefCell<Option<ConfigError>> = RefCell::new(None);
        let resolved_value = RE.replace_all(
            input.as_ref(),
            ConfigVariableResolver {
                config_dir: self.config_dir.as_path(),
                config_error_ref: &config_error_ref,
            }).to_string();

        if let Some(config_error) = config_error_ref.take() {
            Err(config_error)
        } else {
            Ok(resolved_value)
        }
    }
}

struct ConfigVariableResolver<'a> {
    config_dir: &'a Path,
    config_error_ref: &'a RefCell<Option<ConfigError>>,
}

impl Replacer for ConfigVariableResolver<'_> {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        // Replace '$$' with '$'.
        if caps.get(1).is_some() {
            dst.push('$');
            return;
        }


        // Replace ${...} with the environment variable value or the file content.
        let name = caps.get(3).unwrap().as_str();
        match caps.get(2) {
            Some(_) => self.append_file(name, dst),
            None => self.append_env_var(name, dst)
        }
    }
}

impl ConfigVariableResolver<'_> {
    fn append_env_var(&mut self, name: &str, dst: &mut String) {
        match env::var(name) {
            Ok(value) => {
                dst.push_str(value.as_str());
            }
            Err(cause) => {
                self.set_config_error(ConfigError::UnresolvedEnvironmentVariable {
                    name: String::from(name),
                    cause,
                });
            }
        }
    }

    fn append_file(&mut self, path: &str, dst: &mut String) {
        let path = {
            let mut buf = PathBuf::from(self.config_dir);
            buf.push(path);
            buf
        };

        match fs::read_to_string(path.as_path()) {
            Ok(content) => {
                dst.push_str(content.trim_end());
            }
            Err(cause) => {
                self.set_config_error(ConfigError::UnresolvedFileVariable {
                    path: path.to_str().unwrap().to_string(),
                    cause,
                });
            }
        }
    }

    fn set_config_error(&mut self, config_error: ConfigError) {
        let cell = self.config_error_ref;
        if cell.borrow().is_none() {
            cell.replace(Some(config_error));
        }
    }
}
