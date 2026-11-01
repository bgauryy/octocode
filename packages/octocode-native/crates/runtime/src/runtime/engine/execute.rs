use super::admission::AdmittedRows;
use super::channel::{ChannelRun, OutputSettings};
use super::page_replay::{Refusal, RefusalScope, page_replay_key, snapshot_expired};
use super::{Channel, RuntimeError, ToolRuntime, runtime_execution_error};
use crate::contracts;
use crate::id::ToolId;
use crate::response::outcome::ToolOutcome;
use crate::response::page_model::ResponsePageOptions;
use octocode_contract_types::ErrorCode;
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};

impl ToolRuntime {
    pub fn admit(
        &self,
        request_id: String,
    ) -> Result<crate::execution::RequestAdmission, RuntimeError> {
        self.requests
            .admit(request_id)
            .map_err(runtime_execution_error)
    }

    /// [`Self::admit`] with the host's per-request options
    /// (`{githubToken?}`). A supplied token is the request's only GitHub
    /// credential; a request without one resolves credentials as before.
    pub fn admit_with(
        &self,
        request_id: String,
        options: Option<Value>,
    ) -> Result<crate::execution::RequestAdmission, RuntimeError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct RequestOptions {
            github_token: Option<String>,
        }
        let token = match options {
            None | Some(Value::Null) => None,
            // Never echo the options: they carry a secret.
            Some(value) => serde_json::from_value::<RequestOptions>(value)
                .map_err(|_| RuntimeError::new(ErrorCode::InvalidInput, "Invalid request options"))?
                .github_token
                .map(|token| {
                    if token.trim().is_empty() {
                        Err(RuntimeError::new(
                            ErrorCode::InvalidInput,
                            "Request option githubToken is empty",
                        ))
                    } else {
                        Ok(SecretString::from(token))
                    }
                })
                .transpose()?,
        };
        let admission = self.admit(request_id)?;
        Ok(match token {
            Some(token) => admission.with_github_token(token),
            None => admission,
        })
    }

    /// Run one admitted call and answer in `channel`. Every interface
    /// (CLI, warm server, MCP) executes through here; a rejected call
    /// projects once through [`super::super::error::tool_error_result`].
    pub async fn execute(
        &self,
        admission: crate::execution::RequestAdmission,
        tool: String,
        input: Value,
        channel: Channel,
    ) -> Result<ToolOutcome, RuntimeError> {
        self.execute_channel(admission, tool, input, channel == Channel::Mcp)
            .await
    }

    async fn execute_channel(
        &self,
        admission: crate::execution::RequestAdmission,
        tool: String,
        input: Value,
        mcp: bool,
    ) -> Result<ToolOutcome, RuntimeError> {
        let id = match self.admit_tool(&tool, mcp) {
            Ok(id) => id,
            // A setting the call lacks is reported after its input: an
            // invalid query fails as invalidInput whatever is configured.
            Err(error) => {
                if error.code == ErrorCode::MissingConfiguration
                    && let Some(id) = ToolId::from_name(&tool)
                    && let Err(invalid) = self.admit_queries(id, &input, false, mcp)
                    && invalid.code == ErrorCode::InvalidInput
                {
                    return Err(invalid);
                }
                return Err(error);
            }
        };
        if id == ToolId::GhCloneRepo && input.get("cursor").is_some() {
            return Err(RuntimeError::new(
                ErrorCode::InvalidInput,
                "ghCloneRepo does not retain saved responses; provide queries for a fresh checkout.",
            ));
        }
        let clasify = id.is_clasify();
        let access = page_replay_key("", mcp, &[], &[], admission.github_credential());
        let opened = if let Some(token) = input.get("cursor").and_then(Value::as_str) {
            contracts::validate(&tool, input.clone())
                .map_err(|error| super::admission::invalid_input_error(&tool, error))?;
            Some(self.open_cursor(id, token, mcp, admission.github_credential())?)
        } else {
            None
        };
        let resume_identity = opened
            .as_ref()
            .and_then(|opened| opened.github_identity().map(str::to_owned));
        let saved = opened.as_ref().map(super::cursors::Opened::member);
        let fresh = saved.is_none();
        let normalized_input = saved.map_or(&input, |member| &member.input);
        let origin_index = saved.and_then(|member| match &member.work {
            super::cursors::SavedWork::ToolResume { index }
            | super::cursors::SavedWork::ResponsePage { index, .. } => *index,
        });
        let page = saved.and_then(|member| match &member.work {
            super::cursors::SavedWork::ResponsePage { cursor, .. } => Some(cursor),
            super::cursors::SavedWork::ToolResume { .. } => None,
        });
        let mut options = ResponsePageOptions::deserialize(normalized_input).unwrap_or_default();
        // A saved page whose response is no longer retained re-executes its
        // rows, pinned to the page: it answers only when its sources
        // reproduce that response. A judgment is not reproducible, so
        // clasify pages are never re-executed.
        let retained = page.map(|cursor| self.page_replays.get(cursor));
        let pinned = match (page, &retained) {
            (Some(cursor), Some(None)) if !clasify => Some(cursor.clone()),
            _ => None,
        };
        if let Some(cursor) = &pinned {
            options.open(cursor);
        }
        let (queries, rejected_rows, row_indices, response_query, replay_key, replayed) =
            if let (Some(cursor), Some(replay)) = (page, retained.filter(|_| pinned.is_none())) {
                let replay = replay
                    .filter(|replay| replay.access == access)
                    .ok_or_else(|| self.refuse(Refusal::OtherAccess, mcp))?;
                options.open(cursor);
                (
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    replay.response_query.clone(),
                    cursor.key,
                    Some(replay),
                )
            } else {
                let AdmittedRows {
                    queries,
                    rejected: rejected_rows,
                } = self.admit_queries(id, normalized_input, saved.is_some(), mcp)?;
                let mut response_query =
                    json!({"queries": queries.iter().map(|query| &**query).collect::<Vec<_>>()});
                if let Some(chars) = normalized_input.get("responseChars") {
                    response_query["responseChars"] = chars.clone();
                }
                let rejected_at = rejected_rows
                    .iter()
                    .map(|(index, _)| *index)
                    .collect::<Vec<_>>();
                let row_indices =
                    super::super::row_indices::admitted_indices(queries.len(), &rejected_at);
                let key = page_replay_key(
                    &tool,
                    mcp,
                    &queries,
                    &origin_index.map_or_else(
                        || row_indices.clone(),
                        |index| vec![index; row_indices.len()],
                    ),
                    admission.github_credential(),
                );
                if pinned.as_ref().is_some_and(|cursor| cursor.key != key) {
                    return Err(self.refuse(Refusal::RequestChanged, mcp));
                }
                (
                    queries,
                    rejected_rows,
                    row_indices,
                    response_query,
                    key,
                    None,
                )
            };
        let dispatcher = self.dispatcher();
        let run = ChannelRun {
            id,
            mcp,
            options,
            queries,
            rejected_rows,
            row_indices,
            origin_index,
            fresh,
            response_query,
            clasify,
            dispatcher,
            output: OutputSettings {
                redact_emails: self.config.resolved.output.redact_emails,
                auto_page_chars: self.limits.auto_page_chars,
            },
            replayed,
            pinned,
            replays: (self.page_replays.clone(), replay_key),
            section_sessions: self.section_sessions.clone(),
            access,
            cursors: self.cursors.clone(),
            resume_identity,
        };
        self.requests
            .execute_blocking_admitted(admission, move |context| {
                if let Err(error) = run.check_replay_access(&context) {
                    return Ok(Err(error));
                }
                run.run(&context)
            })
            .await
            .map_err(runtime_execution_error)?
    }

    /// The domain execution path for this call's rows.
    pub(super) fn dispatcher(&self) -> super::super::domain_dispatch::DomainDispatcher {
        super::super::domain_dispatch::DomainDispatcher {
            paths: self.paths.clone(),
            security: self.security.clone(),
            github_services: self.github_services.clone(),
            github_cache: self.github_cache.clone(),
            github_access: super::super::github::ResponseAccess::default(),
            config: self.config.clone(),
            limits: self.limits,
            home: self.config.home.clone(),
            handle: tokio::runtime::Handle::current(),
            lsp_pool: self.lsp_pool.clone(),
            local_views: self.local_views.clone(),
            source_views: self.source_views.clone(),
            lsp_execution_config: self.lsp_execution_config.clone(),
            availability: self.availability.clone(),
            selection_available: self.selection_available,
            evaluator: self.evaluator.clone(),
            artifact_cache: self.artifact_cache.clone(),
        }
    }

    /// The saved work `token` resumes for `id` on this surface. A handle
    /// retained under another access identity is told apart: the same
    /// credential on the other surface, else another credential.
    fn open_cursor(
        &self,
        id: ToolId,
        token: &str,
        mcp: bool,
        credential: Option<&octocode_github::ResolvedCredential>,
    ) -> Result<super::cursors::Opened, RuntimeError> {
        let access = |mcp| page_replay_key("", mcp, &[], &[], credential);
        self.cursors
            .open(id, token, access(mcp))
            .map_err(|refusal| match refusal {
                Refusal::OtherAccess if self.cursors.open(id, token, access(!mcp)).is_ok() => {
                    self.refuse(Refusal::OtherSurface, mcp)
                }
                refusal => self.refuse(refusal, mcp),
            })
    }

    /// The error for a cursor this runtime refuses, sent on the `mcp` surface.
    fn refuse(&self, refusal: Refusal, mcp: bool) -> RuntimeError {
        snapshot_expired(
            refusal,
            RefusalScope {
                persistent: self.page_replays.persistent,
                mcp,
            },
        )
    }
}
