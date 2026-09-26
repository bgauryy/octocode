//! OpenCode's existing local server owns the recipient. Routing never starts an agent.
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::time::Duration;

pub fn validate(endpoint: &str) -> Result<()> {
    super::loopback(endpoint, "http", false)
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("OpenCode {error}"))
}

pub struct OpenCode {
    agent: ureq::Agent,
    endpoint: String,
    session: String,
    workspace: String,
    authorization: Option<ureq::http::HeaderValue>,
}
impl OpenCode {
    pub fn connect(endpoint: &str, session: &str, workspace: &str) -> Result<Self> {
        validate(endpoint)?;
        validate_session(session)?;
        let authorization = authorization(endpoint)?;
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(5)))
            .timeout_connect(Some(Duration::from_secs(2)))
            .max_response_header_size(16 * 1024)
            .build()
            .into();
        Ok(Self {
            agent,
            endpoint: endpoint.to_owned(),
            session: session.to_owned(),
            workspace: workspace.to_owned(),
            authorization,
        })
    }
    fn request<B>(&self, request: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        let request = request.query("directory", &self.workspace);
        if let Some(auth) = &self.authorization {
            request.header(ureq::http::header::AUTHORIZATION, auth.clone())
        } else {
            request
        }
    }
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.endpoint.trim_end_matches('/'))
    }
    fn get(&self, path: &str) -> Result<Value> {
        let mut response = self
            .request(self.agent.get(self.url(path)))
            .call()
            .context("OpenCode preflight failed; no message submitted")?;
        if response.status().as_u16() != 200 {
            bail!(
                "OpenCode preflight returned HTTP {}; no message submitted",
                response.status().as_u16()
            );
        }
        Ok(response
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_json()?)
    }
    pub fn idle(&self) -> Result<bool> {
        // Revalidate on every batch: a cached connection does not prove current ownership.
        let session = self.get(&format!("/session/{}", self.session))?;
        let directory = session["directory"]
            .as_str()
            .context("OpenCode session directory missing; no message submitted")?;
        if session["id"] != self.session
            || std::fs::canonicalize(directory)? != std::path::Path::new(&self.workspace)
        {
            bail!("OpenCode session/workspace mismatch; no message submitted");
        }
        let statuses = self.get("/session/status")?;
        let statuses = statuses
            .as_object()
            .context("Invalid OpenCode status map; no message submitted")?;
        // OpenCode omits idle sessions from its status map.
        match statuses.get(&self.session) {
            None => Ok(true),
            Some(status) => match status["type"].as_str() {
                Some("idle") => Ok(true),
                Some("busy" | "retry") => Ok(false),
                _ => bail!("Unknown OpenCode runtime status; no message submitted"),
            },
        }
    }
    pub fn submit(&self, content: &str, action: bool) -> Result<()> {
        let suffix = if action { "prompt_async" } else { "message" };
        let url = self.url(&format!("/session/{}/{suffix}", self.session));
        // Preserve host configuration and let OpenCode allocate chronological message IDs.
        let mut response = self
            .request(self.agent.post(url))
            .send_json(json!({"noReply":!action,"parts":[{"type":"text","text":content}]}))
            .context(
                "OpenCode submission failed; outcome may be uncertain; inspect before retrying",
            )?;
        let status = response.status().as_u16();
        if action {
            if status != 204 {
                bail!(
                    "OpenCode asynchronous submission returned HTTP {status}; inspect before retrying"
                );
            }
        } else {
            if status != 200 {
                bail!(
                    "OpenCode passive submission returned HTTP {status}; inspect before retrying"
                );
            }
            let receipt: Value = response.body_mut().with_config().limit(1024 * 1024).read_json()
                .context("OpenCode receipt was invalid; outcome may be uncertain; inspect before retrying")?;
            if receipt["info"]["sessionID"] != self.session
                || receipt["info"]["role"] != "user"
                || !receipt["info"]["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("msg_"))
                || !receipt["parts"].as_array().is_some_and(|parts| {
                    parts
                        .iter()
                        .any(|part| part["type"] == "text" && part["text"] == content)
                })
            {
                bail!(
                    "OpenCode receipt did not match submitted message/session; inspect before retrying"
                );
            }
        }
        // HTTP acceptance is not a model-read or task-completion acknowledgement.
        Ok(())
    }
}
fn authorization(endpoint: &str) -> Result<Option<ureq::http::HeaderValue>> {
    let Ok(password) = std::env::var("OPENCODE_SERVER_PASSWORD") else {
        return Ok(None);
    };
    if password.is_empty() {
        return Ok(None);
    }
    if std::env::var("OCTOCODE_OPENCODE_AUTH_ENDPOINT")
        .ok()
        .as_deref()
        != Some(endpoint)
    {
        bail!(
            "Set OCTOCODE_OPENCODE_AUTH_ENDPOINT to the exact attached endpoint before using OPENCODE_SERVER_PASSWORD"
        );
    }
    let username = std::env::var("OPENCODE_SERVER_USERNAME").unwrap_or_else(|_| "opencode".into());
    if username.is_empty()
        || username.contains(':')
        || username.chars().any(char::is_control)
        || password.chars().any(char::is_control)
        || username.len() + password.len() > 8192
    {
        bail!("Invalid OpenCode authentication configuration");
    }
    let mut header = ureq::http::HeaderValue::from_str(&format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    ))?;
    header.set_sensitive(true);
    Ok(Some(header))
}

pub fn validate_session(session: &str) -> Result<()> {
    // Opaque IDs cannot become paths, query strings, or request headers.
    if !session.starts_with("ses_")
        || session.len() <= 4
        || session.len() > 256
        || !session
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        bail!("OpenCode vendorSession must be its existing ses_ identifier");
    }
    Ok(())
}
