//! `Fallback`: resolving definitions, then reading direct bases and
//! subtypes from syntax.
use super::*;
use crate::tools::lsp_search::importers::{CONCURRENT_FILE_VERIFICATIONS, ordered_concurrently};

impl<'a, 'p> Fallback<'a, 'p> {
    pub(super) async fn definitions(
        &self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<Value>, LspFailure> {
        let mut found = get_locations(
            self.client,
            self.snippet_policy,
            self.cancel,
            LocationRequest::Definition,
            path,
            line,
            character,
        )
        .await?;
        let mut nodes = Vec::new();
        let mut unresolved = false;
        for snippet in &found {
            let target = uri_to_path(&snippet.uri);
            match self
                .node(
                    &target,
                    snippet.range.start.line,
                    snippet.range.start.character,
                )
                .await
            {
                Ok(node) => nodes.push(node),
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(_) => unresolved = true,
            }
        }
        if !unresolved && !nodes.is_empty() {
            return Ok(nodes);
        }
        if unresolved {
            let (resolved, warnings, _) = {
                let mut sources = self.sources.lock().await;
                resolve_definition_chain(
                    self.client,
                    &mut sources,
                    self.snippet_policy,
                    self.cancel,
                    path,
                    line,
                    character,
                )
                .await?
            };
            found = resolved;
            for warning in warnings {
                self.fail(warning).await;
            }
        }
        if found.is_empty() {
            self.fail(format!(
                "No definition establishes the heritage relation at {path}:{}:{}",
                line + 1,
                character + 1
            ))
            .await;
        }
        nodes.clear();
        for found in found {
            self.cancel.check().map_err(LspFailure::cancelled)?;
            let target = uri_to_path(&found.uri);
            match self
                .node(&target, found.range.start.line, found.range.start.character)
                .await
            {
                Ok(node) => nodes.push(node),
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(failure) => self.fail_from(failure).await,
            }
        }
        Ok(nodes)
    }

    pub(super) async fn bases(&self, node: &Value) -> Result<Vec<Value>, LspFailure> {
        let key = self.keys.lock().await.key(node);
        if let Some(bases) = self.bases.lock().await.get(&key).cloned() {
            return Ok(bases);
        }
        let path = node_path(node)?;
        let source = self.source(&path).await?;
        let document = self.document(&path).await?;
        let bases = heritage::direct_bases(&document, &source.content, &path, node)
            .map_err(LspFailure::invalid_query)?;
        let mut resolved = Vec::new();
        for base in bases {
            match self.definitions(&path, base.line, base.character).await {
                Ok(nodes) => resolved.extend(nodes),
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(failure) => self.fail_from(failure).await,
            }
        }
        self.bases.lock().await.insert(key, resolved.clone());
        Ok(resolved)
    }

    pub(super) async fn subtypes(&self, parent: &Value) -> Result<Vec<Value>, LspFailure> {
        // Nothing implements an impl block; asking the server at its self
        // type would list that type's own impls.
        if parent.get("kind").and_then(Value::as_str) == Some("impl") {
            return Ok(Vec::new());
        }
        let path = node_path(parent)?;
        let (line, character) = node_position(parent)?;
        let parent_key = self.keys.lock().await.key(parent);
        let seed = self.initial.lock().await.take();
        let (candidates, anchor) = match seed {
            Some((root, candidates)) if self.keys.lock().await.key(&root) == parent_key => {
                (candidates, true)
            }
            _ => (
                get_locations(
                    self.client,
                    self.snippet_policy,
                    self.cancel,
                    LocationRequest::Implementation,
                    &path,
                    line,
                    character,
                )
                .await?,
                false,
            ),
        };
        // The anchor already owns a checked CandidateWindow. A deeper type
        // checks one window here and resumes as an anchor for the rest.
        if !anchor && candidates.len() > MAX_HIERARCHY_NODES {
            self.candidate_capped.lock().await.push(parent.clone());
        }
        // Each candidate resolves to its declaration, then that declaration's
        // bases are read; both steps keep candidate order, so a server that
        // pipelines requests (tsgo) takes several candidates at once with the
        // same answer.
        let resolved = self
            .each(
                candidates.into_iter().take(MAX_HIERARCHY_NODES).collect(),
                |candidate| async move {
                    let path = uri_to_path(&candidate.uri);
                    let (line, character) =
                        (candidate.range.start.line, candidate.range.start.character);
                    Ok((
                        path,
                        line,
                        character,
                        self.resolve_candidate(&candidate).await?,
                    ))
                },
            )
            .await?;
        let mut checked = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (path, line, character, resolved) in resolved {
            let resolved = match resolved {
                Ok(resolved) => resolved,
                Err(failure) => {
                    self.fail_from(failure).await;
                    continue;
                }
            };
            let mut distinct = {
                let mut keys = self.keys.lock().await;
                let mut identities = std::collections::HashSet::new();
                resolved
                    .into_iter()
                    .filter(|node| identities.insert(keys.key(node)))
                    .collect::<Vec<_>>()
            };
            if distinct.len() > 1 {
                self.fail(format!(
                    "Implementation candidate at {path}:{}:{} resolves to {} distinct type declarations; its immediate subtype identity is ambiguous",
                    line + 1,
                    character + 1,
                    distinct.len()
                )).await;
                continue;
            }
            let Some(candidate) = distinct.pop() else {
                continue;
            };
            let identity = self.keys.lock().await.key(&candidate);
            if identity == parent_key || !seen.insert(identity) {
                continue;
            }
            checked.push(candidate);
        }
        let based = self
            .each(checked, |candidate| async move {
                match self.bases(&candidate).await {
                    Err(failure) if failure.is_cancelled() => Err(failure),
                    bases => Ok((candidate, bases)),
                }
            })
            .await?;
        let mut direct = Vec::new();
        for (candidate, bases) in based {
            match bases {
                Ok(bases) => {
                    let mut keys = self.keys.lock().await;
                    if bases.iter().any(|base| keys.key(base) == parent_key) {
                        direct.push(candidate);
                    } else if bases.is_empty() {
                        self.fail(format!("No resolved immediate heritage establishes the implementation candidate {}", candidate["name"])).await;
                    }
                }
                Err(failure) => self.fail_from(failure).await,
            }
        }
        Ok(direct)
    }

    /// The declarations `candidate` (an implementation answer) names: the
    /// trait implementation located there, else the type it resolves to.
    /// A failure other than cancellation is the candidate's verdict.
    async fn resolve_candidate(
        &self,
        candidate: &LspCodeSnippet,
    ) -> Result<Result<Vec<Value>, LspFailure>, LspFailure> {
        self.cancel.check().map_err(LspFailure::cancelled)?;
        let path = uri_to_path(&candidate.uri);
        let (line, character) = (candidate.range.start.line, candidate.range.start.character);
        // A trait implementation is the subtype itself, not the type its
        // self-type position resolves to (which declares no heritage).
        let resolved = match self.impl_node(&path, line, character).await {
            Ok(Some(node)) => Ok(vec![node]),
            Err(failure) if failure.is_cancelled() => return Err(failure),
            _ => self.resolved_nodes(&path, line, character).await,
        };
        match resolved {
            Err(failure) if failure.is_cancelled() => Err(failure),
            resolved => Ok(resolved),
        }
    }

    /// `task` over `items`, results in item order: several at once on a
    /// server that pipelines requests, one at a time otherwise (tsserver
    /// works through requests in turn, so queued ones only wait against
    /// their timeouts). Cancellation ends the run.
    async fn each<I, T, Fut>(
        &self,
        items: Vec<I>,
        mut task: impl FnMut(I) -> Fut,
    ) -> Result<Vec<T>, LspFailure>
    where
        Fut: std::future::Future<Output = Result<T, LspFailure>>,
    {
        if self.client.serves_requests_concurrently() {
            return ordered_concurrently(items, CONCURRENT_FILE_VERIFICATIONS, task).await;
        }
        let mut done = Vec::with_capacity(items.len());
        for item in items {
            done.push(task(item).await?);
        }
        Ok(done)
    }
}
