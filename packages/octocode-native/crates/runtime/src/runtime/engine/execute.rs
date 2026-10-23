use super::admission::{AdmittedRows, output_contract_error};
use super::channel::{ChannelRun, OutputSettings};
use super::page_replay::page_replay_key;
use super::{Channel, LIGHT_FILE_BYTES, RuntimeError, ToolRuntime, runtime_execution_error};
use crate::contracts;
use crate::execution::Cost;
use crate::id::{ToolFamily, ToolId};
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
    /// (CLI, warm server, N-API/MCP) executes through here; a rejected call
    /// projects once through [`super::super::error::tool_error_result`].
    pub async fn execute(
        &self,
        admission: crate::execution::RequestAdmission,
        tool: String,
        input: Value,
        channel: Channel,
    ) -> Result<ToolOutcome, RuntimeError> {
        let input = contracts::normalize_input(&tool, input);
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
                    && let Err(invalid) = self.admit_queries(id, &input)
                    && invalid.code == ErrorCode::InvalidInput
                {
                    return Err(invalid);
                }
                return Err(error);
            }
        };
        let clasify = id.is_clasify();
        // Parse response-paging options before contract validation.
        let mut options = ResponsePageOptions::deserialize(&input).unwrap_or_default();
        let access = page_replay_key(&tool, mcp, &[], &[], admission.github_credential());
        let (queries, rejected_rows, row_indices, response_query, replay_key, replayed) =
            if options.response_snapshot.is_some() {
                let (key, replay) = self.open_saved(&tool, &input, &mut options, access)?;
                (
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    replay.response_query.clone(),
                    key,
                    Some(replay),
                )
            } else {
                if clasify && options.response_scope.is_none() {
                    options.response_scope = Some(crate::response::cursor::PageScope::Rows);
                }
                let AdmittedRows {
                    queries,
                    rejected: rejected_rows,
                } = self.admit_queries(id, &input)?;
                let response_query = match queries.as_slice() {
                    [query] => (**query).clone(),
                    queries => {
                        json!({"queries": queries.iter().map(|query| &**query).collect::<Vec<_>>()})
                    }
                };
                let rejected_at: Vec<usize> =
                    rejected_rows.iter().map(|(index, _)| *index).collect();
                let row_indices =
                    super::super::row_indices::admitted_indices(queries.len(), &rejected_at);
                let key = page_replay_key(
                    &tool,
                    mcp,
                    &queries,
                    &row_indices,
                    admission.github_credential(),
                );
                (
                    queries,
                    rejected_rows,
                    row_indices,
                    response_query,
                    key,
                    None,
                )
            };
        let cost = self.request_cost(id, &queries, replayed.is_some());
        let dispatcher = self.dispatcher();
        let run = ChannelRun {
            id,
            mcp,
            options,
            queries,
            rejected_rows,
            row_indices,
            response_query,
            clasify,
            dispatcher,
            output: OutputSettings {
                redact_emails: self.config.resolved.output.redact_emails,
                auto_page_chars: self.limits.auto_page_chars,
            },
            replayed,
            replays: (self.page_replays.clone(), replay_key),
            section_sessions: self.section_sessions.clone(),
            access,
        };
        self.requests
            .execute_blocking_admitted(admission.with_cost(cost), move |context| {
                if let Err(error) = run.check_replay_access(&context) {
                    return Ok(Err(error));
                }
                run.run(&context)
                    .map(|result| result.map_err(|error| output_contract_error(&tool, error)))
            })
            .await
            .map_err(runtime_execution_error)?
    }

    /// The admission [`Cost`] of `queries`: light for a provider page the
    /// memo serves (no dispatch, no source check) or for local rows the local
    /// runner serves that each read one regular file of at most
    /// [`LIGHT_FILE_BYTES`] and judge nothing. Anything else may walk a tree,
    /// or wait on a provider (a `clasifyContext` or `selection` row asks the
    /// classification provider) or a language server.
    pub(super) fn request_cost(
        &self,
        id: ToolId,
        queries: &[crate::security::CheckedQuery],
        replayed: bool,
    ) -> Cost {
        if replayed {
            return Cost::Light;
        }
        let light_row = |row: &Value| {
            let judged = row.get("clasifyContext").is_some() || row.get("selection").is_some();
            !judged
                && row["path"].as_str().is_some_and(|path| {
                    std::fs::metadata(self.paths.expand_and_resolve(std::path::Path::new(path)))
                        .is_ok_and(|meta| meta.is_file() && meta.len() <= LIGHT_FILE_BYTES)
                })
        };
        let light = match id.family() {
            ToolFamily::GitHub | ToolFamily::Remote => replayed,
            ToolFamily::Local => {
                id != ToolId::LspSearch
                    && !queries.is_empty()
                    && queries.iter().all(|query| light_row(query))
            }
        };
        if light { Cost::Light } else { Cost::Walk }
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
            lsp_execution_config: self.lsp_execution_config.clone(),
            availability: self.availability.clone(),
            selection_available: self.selection_available,
            evaluator: self.evaluator.clone(),
            artifact_cache: self.artifact_cache.clone(),
        }
    }
}
