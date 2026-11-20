//! Issued handles for saved tool work and immutable response pages.
//! A handle names one retained continuation by its content ([`handle`]). It
//! never contains caller-editable progress.
use super::page_replay::{Refusal, ReplayScope};
use crate::runtime::row_indices::RowOrigin;
use octocode_runtime_base::cache::{CacheConfig, CacheKey, Disk, Store};
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError, StorageFault};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::cursor::{Cursor, issued};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The shaping inputs a response-page fallback must reproduce after its
/// body is evicted. Rejected rows stay on page one, but their space and the
/// original repair notes remain part of the admitted rows' page plan.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct PageShape {
    pub beside_rows: usize,
    pub repairs: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum SavedWork {
    ToolResume {
        index: Option<usize>,
    },
    /// The walks of several rows on one handle: the input's queries, each
    /// continuing the query at the same position of `indices`.
    Rows {
        indices: Vec<usize>,
    },
    /// A later page of a saved response, and the query index its rows
    /// inherit (`rows` when they answer several), so a replay that is no
    /// longer retained re-executes the same rows under the same indices
    /// ([`Cursor::key`] checks both).
    ResponsePage {
        cursor: Cursor,
        shape: PageShape,
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rows: Option<Vec<usize>>,
    },
}

impl SavedWork {
    /// The query each row of this work answers.
    pub(super) fn origin(&self) -> RowOrigin {
        match self {
            Self::Rows { indices }
            | Self::ResponsePage {
                rows: Some(indices),
                ..
            } => RowOrigin::Rows(indices.clone()),
            Self::ToolResume { index: Some(index) }
            | Self::ResponsePage {
                index: Some(index), ..
            } => RowOrigin::Query(*index),
            Self::ToolResume { index: None } | Self::ResponsePage { .. } => RowOrigin::Request,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Member {
    pub tool: String,
    /// Complete normalized input, including contract-typed private progress.
    pub input: Value,
    pub work: SavedWork,
    /// The digest of the provider evidence a resumed tool continuation
    /// answers from ([`evidence`]).
    pub evidence: Option<String>,
}

/// One issued continuation as the registry keeps it: the access identity
/// it was issued to and the work it resumes.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Issued {
    /// The scope it is kept under ([`ReplayScope::of`] its tool).
    #[serde(default)]
    scope: String,
    access: [u8; 32],
    pub github_identity: Option<String>,
    member: Member,
}

pub(super) struct Registry {
    store: Store<Issued>,
    /// What issued tool continuations own, beside the handles (same
    /// directory and retention).
    evidence: Store<evidence::Sealed>,
    /// The saved answers' directory, whose body files evidence links.
    answers: Option<Disk>,
    /// What they own that may not be stored
    /// ([`octocode_runtime_base::providers::saved_answers::gate_keeps`]): this process's
    /// memory only ([`evidence`]).
    held_evidence: Store<octocode_github::ProviderEvidence>,
    policy: ReplayScope,
    disk: Option<PathBuf>,
    /// Taken before the sidecar file lock, so that lock is contended across
    /// processes only.
    mutation: octocode_io::lock::Turnstile,
}

/// The continuations one output issues, retained together before it is
/// published.
pub(super) struct Pending {
    policy: ReplayScope,
    access: [u8; 32],
    github_identity: Option<String>,
    issued: BTreeMap<String, Member>,
    /// The tool whose continuations own the call's provider evidence, and
    /// the evidence.
    evidence: Option<(ToolId, evidence::Owned)>,
    /// The call's evidence was taken (it may have held nothing to own).
    sealed: bool,
}

pub(super) struct Opened {
    issued: Arc<Issued>,
}

impl Opened {
    pub(super) fn member(&self) -> &Member {
        &self.issued.member
    }
    pub(super) fn github_identity(&self) -> Option<&str> {
        self.issued.github_identity.as_deref()
    }
}

mod evidence;
mod walks;

impl Registry {
    /// A registry whose every tool keeps its handles under one `policy`:
    /// what tests build to stand for another runtime's registry.
    #[cfg(test)]
    pub(super) fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        Self::scoped(config, disk, policy.into())
    }

    /// A registry that keeps each tool's handles under its scope
    /// ([`ReplayScope::of`]).
    pub(super) fn scoped(config: CacheConfig, disk: Option<PathBuf>, policy: ReplayScope) -> Self {
        Self {
            store: Store::new(config.clone(), disk.clone()),
            evidence: Store::new(config.clone(), disk.clone()),
            answers: None,
            held_evidence: Store::new(config, None),
            policy,
            disk,
            mutation: octocode_io::lock::Turnstile::new(),
        }
    }

    pub(super) fn pending(&self, access: [u8; 32], github_identity: Option<String>) -> Pending {
        Pending {
            policy: self.policy.clone(),
            access,
            github_identity,
            issued: BTreeMap::new(),
            evidence: None,
            sealed: false,
        }
    }

    /// Evidence links the bodies of the saved answers in `directory`.
    pub(super) fn linking_answers(mut self, directory: Option<PathBuf>) -> Self {
        self.answers = directory.map(Disk::new);
        self
    }

    /// Where a handle is kept: under the scope of the tool it was issued to.
    fn key(scope: &str, token: &str) -> CacheKey {
        CacheKey {
            namespace: "issued-continuations-v2".into(),
            scope: scope.into(),
            resource: token.into(),
        }
    }

    pub(super) fn clear_memory(&self) {
        self.store.clear_memory();
        self.evidence.clear_memory();
        self.held_evidence.clear_memory();
    }

    /// Every handle in one output becomes reachable atomically, before
    /// publication. A handle names its content ([`handle`]), so one an
    /// earlier output issued is the same continuation: retaining it again
    /// refreshes its retention.
    pub(super) fn retain(
        &self,
        pending: Pending,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        if pending.issued.is_empty() {
            return Ok(());
        }
        let _mutation = self.mutation_lock(context)?;
        let _storage = self.process_lock(context)?;
        context.check()?;
        if let Some((owner, owned)) = pending.evidence
            && pending
                .issued
                .values()
                .any(|member| member.evidence.as_ref() == Some(&owned.digest))
        {
            let key = evidence::key(
                pending.policy.of(owner),
                pending.github_identity.as_deref(),
                &owned.digest,
            );
            evidence::retain(
                &self.evidence,
                &self.held_evidence,
                self.answers.as_ref(),
                key,
                owned,
            );
        }
        for (token, member) in pending.issued {
            let scope = ToolId::from_name(&member.tool)
                .map_or(pending.policy.workspace.as_str(), |tool| {
                    pending.policy.of(tool)
                })
                .to_owned();
            let key = Self::key(&scope, &token);
            if self
                .store
                .get(&key)
                .is_some_and(|current| current.value.access != pending.access)
            {
                return Err(ExecutionError::WorkerFailed);
            }
            // A clasify cursor names a plan kept that long: it must not die first.
            let retention = super::page_replay::retention_of(&member.tool);
            let issued = Issued {
                scope,
                access: pending.access,
                github_identity: pending.github_identity.clone(),
                member,
            };
            let bytes = octocode_engine::json_size::bytes(&issued).unwrap_or(usize::MAX);
            let (_, stored) = self.store.put_checked(
                octocode_runtime_base::execution::cancel::deadline_of(context),
                key,
                issued,
                bytes,
                retention,
            );
            if let Err(cause) = stored {
                // An entry the store refuses outright is no storage failure; a lock
                // or a write the filesystem refuses is (full, over quota, read-only).
                return Err(match cause {
                    octocode_runtime_base::cache::NotStored::Cancelled => ExecutionError::Cancelled,
                    octocode_runtime_base::cache::NotStored::Refused => {
                        ExecutionError::WorkerFailed
                    }
                    octocode_runtime_base::cache::NotStored::Unlocked(kind) => {
                        ExecutionError::StorageUnavailable(
                            kind.map_or(StorageFault::Other, Into::into),
                        )
                    }
                    octocode_runtime_base::cache::NotStored::Failed(kind) => {
                        ExecutionError::StorageUnavailable(kind.into())
                    }
                });
            }
        }
        Ok(())
    }

    fn mutation_lock(
        &self,
        context: &ExecutionContext,
    ) -> Result<octocode_io::lock::Turn<'_>, ExecutionError> {
        self.mutation.take(
            octocode_runtime_base::execution::cancel::deadline_of(context),
            octocode_io::lock::LOCK_POLL,
            || context.check(),
        )
    }

    fn process_lock(
        &self,
        context: &ExecutionContext,
    ) -> Result<Option<octocode_io::lock::LockedFile>, ExecutionError> {
        let Some(disk) = &self.disk else {
            return Ok(None);
        };
        octocode_io::atomic::create_private_dir_all(disk)
            .map_err(|error| ExecutionError::StorageUnavailable((&error).into()))?;
        let file = octocode_io::lock::open_lock_file(&disk.join("continuations.lock"))
            .map_err(|error| ExecutionError::StorageUnavailable((&error).into()))?;
        // One stable sidecar bounds lock files and serializes short merges.
        loop {
            context.check()?;
            let wait = octocode_runtime_base::execution::cancel::deadline_of(context)
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            match octocode_io::lock::lock_within(
                file.try_clone()
                    .map_err(|error| ExecutionError::StorageUnavailable((&error).into()))?,
                wait,
                octocode_io::lock::LOCK_POLL,
            ) {
                Ok(lock) => return Ok(Some(lock)),
                Err(octocode_io::lock::LockError::Busy) => {}
                Err(octocode_io::lock::LockError::Io(error)) => {
                    return Err(ExecutionError::StorageUnavailable((&error).into()));
                }
            }
        }
    }

    /// The work `token` resumes for `tool` under `access`, or why it does
    /// not open here.
    pub(super) fn open(
        &self,
        tool: ToolId,
        token: &str,
        access: [u8; 32],
    ) -> Result<Opened, Refusal> {
        issued::parts(token).ok_or(Refusal::Malformed)?;
        // A handle of the other kind of tool is kept under the other scope:
        // found there, it was issued to another tool.
        let issued = [self.policy.of(tool), self.policy.other_than(tool)]
            .into_iter()
            .find_map(|scope| self.store.get(&Self::key(scope, token)))
            .ok_or(Refusal::NotRetained)?
            .value;
        if issued.access != access {
            return Err(Refusal::OtherAccess);
        }
        if issued.member.tool != tool.as_str() {
            return Err(Refusal::OtherTool);
        }
        Ok(Opened { issued })
    }

    /// The provider evidence `opened` owns while it is retained, and the
    /// digest it is retained under.
    pub(super) fn evidence(
        &self,
        opened: &Opened,
    ) -> Option<(String, Arc<octocode_github::ProviderEvidence>)> {
        let digest = opened.issued.member.evidence.as_deref()?;
        let evidence = evidence::open(
            &self.evidence,
            &self.held_evidence,
            &evidence::key(&opened.issued.scope, opened.github_identity(), digest),
        )?;
        Some((digest.to_owned(), evidence))
    }
}

impl Pending {
    /// The provider evidence `tool`'s own tool continuations issued by
    /// this output own ([`evidence::owned`]).
    pub(super) fn own_evidence(
        &mut self,
        tool: ToolId,
        observed: octocode_github::ProviderEvidence,
    ) {
        self.evidence = evidence::owned(observed).map(|owned| (tool, owned));
        self.sealed = true;
    }

    /// `tool`'s own tool continuations issued by this output own the evidence
    /// already retained under `digest`.
    pub(super) fn keep_evidence(&mut self, tool: ToolId, digest: &str) {
        self.evidence = Some((tool, evidence::Owned::retained(digest)));
        self.sealed = true;
    }

    pub(super) fn owns_evidence(&self) -> bool {
        self.sealed
    }

    pub(super) fn is_empty(&self) -> bool {
        self.issued.is_empty()
    }

    pub(super) fn bind_github_identity(&mut self, identity: Option<String>) {
        self.github_identity = identity;
    }

    /// The handle of `work` resuming `tool` with `input`, issued once per
    /// output; the same continuation always has the same handle
    /// ([`handle`]).
    pub(super) fn insert(&mut self, tool: ToolId, mut input: Value, work: SavedWork) -> String {
        shown_paths(&mut input);
        let token = handle(self.policy.of(tool), &self.access, tool, &input, &work);
        let evidence = match (&self.evidence, &work) {
            (Some((owner, owned)), SavedWork::ToolResume { .. } | SavedWork::Rows { .. })
                if *owner == tool =>
            {
                Some(owned.digest.clone())
            }
            _ => None,
        };
        self.issued.entry(token.clone()).or_insert(Member {
            tool: tool.as_str().into(),
            input,
            work,
            evidence,
        });
        token
    }
}

/// The paths a saved continuation names, as the shown (redacted) names: a
/// file named after a credential is stored with the placeholder in its place,
/// and resuming resolves the shown name back to the entry
/// ([`octocode_runtime_base::policy::path`]), as a localFetch continuation does.
fn shown_paths(input: &mut Value) {
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let show = |text: &mut Value| {
        if let Some(shown) = text.as_str().and_then(|text| security.sanitize_leaf(text)) {
            *text = Value::String(shown);
        }
    };
    let row = |row: &mut Value| {
        let Some(fields) = row.as_object_mut() else {
            return;
        };
        if let Some(path) = fields.get_mut("path") {
            show(path);
        }
        if let Some(paths) = fields.get_mut("paths").and_then(Value::as_array_mut) {
            paths.iter_mut().for_each(show);
        }
    };
    row(input);
    if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
        queries.iter_mut().for_each(row);
    }
}

/// The opaque handle of one continuation: a digest of the replay policy,
/// access identity (the call's credential scope), tool, complete canonical
/// input and saved work (a response page's replay key). The policy binds
/// configuration, contract and planner epoch, and the workspace for a tool
/// bound to it ([`ReplayScope::of`]), so reissuing work cannot resurrect an
/// older policy's token. Within one policy the same
/// continuation has one handle, so repeated leads are read once.
fn handle(
    policy: &str,
    access: &[u8; 32],
    tool: ToolId,
    input: &Value,
    work: &SavedWork,
) -> String {
    use octocode_engine::digest::{Nulls, canonical_json};
    // `SavedWork` derives `Serialize` over integers, byte arrays and a
    // `Cursor` of the same, with no map keys or custom impls, so
    // `to_value` cannot fail and the default never stands in for a work.
    let identity = serde_json::json!([
        "issued-continuation",
        policy,
        hex::encode(access),
        tool.as_str(),
        canonical_json(input.clone(), Nulls::Keep),
        canonical_json(serde_json::to_value(work).unwrap_or_default(), Nulls::Keep),
    ]);
    let digest = octocode_engine::digest::json_sha256(&identity)
        .unwrap_or_else(|| octocode_engine::digest::sha256_digest(&[]));
    let (set, member) = digest.split_at(issued::ID_BYTES);
    let part = |bytes: &[u8]| {
        let mut id = [0; issued::ID_BYTES];
        id.copy_from_slice(bytes);
        issued::encode(&id)
    };
    issued::format(&part(set), &part(member))
}

#[cfg(test)]
mod tests;
