use crate::config::GithubConfig;
use once_cell::sync::Lazy;
use std::error::Error;
use std::time::Duration;
use ureq::{serde_json, Agent, AgentBuilder};

pub struct GithubClient {
    config: GithubConfig,
    agent: Agent,
}

#[derive(Debug)]
pub struct WorkflowRun {
    pub url: String,
}

impl GithubClient {
    pub fn new(config: &GithubConfig) -> GithubClient {
        static USER_AGENT: Lazy<String> = Lazy::new(|| {
            let mut buf = String::new();
            buf.push_str(env!("CARGO_PKG_NAME"));
            buf.push('/');
            buf.push_str(env!("VERGEN_GIT_DESCRIBE"));
            buf
        });

        GithubClient {
            config: config.clone(),
            agent: AgentBuilder::new()
                .timeout(Duration::from_secs(10))
                .user_agent(&USER_AGENT)
                .build(),
        }
    }

    pub fn fetch_queued_workflow_runs(&self) -> Result<Vec<WorkflowRun>, Box<dyn Error>> {
        let request_url = {
            let mut buf = String::new();
            buf.push_str(&self.config.runners.api_endpoint_url);
            buf.push_str("/repos/");
            buf.push_str(&self.config.runners.repo_user);
            buf.push('/');
            buf.push_str(&self.config.runners.repo_name);
            buf.push_str("/actions/runs?status=queued");
            buf
        };

        let res: serde_json::Value = self
            .agent
            .get(&request_url)
            .set("Accept", "application/vnd.github+json")
            .set(
                "Authorization",
                &format!("Bearer {}", self.config.personal_access_token),
            )
            .set("X-GitHub-Api-Version", "2022-11-28")
            .set("Accept-Encoding", "br, gzip")
            .call()?
            .into_json()?;

        if let Some(array) = res["workflow_runs"].as_array() {
            let mut is_ok = true;
            let runs = array
                .iter()
                .flat_map(|run| {
                    if let Some(url) = run["url"].as_str() {
                        Some(WorkflowRun {
                            url: url.to_string(),
                        })
                    } else {
                        is_ok = false;
                        None
                    }
                })
                .collect();

            if is_ok {
                Ok(runs)
            } else {
                Err("The response contains a run without the 'url' field.".into())
            }
        } else {
            Err("The response doesn't have an array field 'workflow_runs'.".into())
        }
    }
}
