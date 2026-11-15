//! Issued handles for saved tool work and immutable response pages.
//! A handle names one retained continuation by its content ([`handle`]). It
//! never contains caller-editable progress.
use super::page_replay::Refusal;
use crate::cache::{CacheConfig, CacheKey, Disk, Store};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::cursor::{Cursor, issued};
use crate::runtime::row_indices::RowOrigin;
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
    pub fn origin(&self) -> RowOrigin {
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
    /// ([`crate::providers::saved_answers::gate_keeps`]): this process's
    /// memory only ([`evidence`]).
    held_evidence: Store<octocode_github::ProviderEvidence>,
    policy: String,
    disk: Option<PathBuf>,
    /// Taken before the sidecar file lock, so that lock is contended across
    /// processes only.
    mutation: octocode_io::lock::Turnstile,
}

/// The continuations one output issues, retained together before it is
/// published.
pub(super) struct Pending {
    policy: String,
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
    pub fn member(&self) -> &Member {
        &self.issued.member
    }
    pub fn github_identity(&self) -> Option<&str> {
        self.issued.github_identity.as_deref()
    }
}

mod evidence;
mod walks;

impl Registry {
    pub fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
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

    pub fn pending(&self, access: [u8; 32], github_identity: Option<String>) -> Pending {
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
    pub fn linking_answers(mut self, directory: Option<PathBuf>) -> Self {
        self.answers = directory.map(Disk::new);
        self
    }

    fn key(&self, token: &str) -> CacheKey {
        CacheKey {
            namespace: "issued-continuations-v2".into(),
            scope: self.policy.clone(),
            resource: token.into(),
        }
    }

    pub fn clear_memory(&self) {
        self.store.clear_memory();
        self.evidence.clear_memory();
        self.held_evidence.clear_memory();
    }

    /// Every handle in one output becomes reachable atomically, before
    /// publication. A handle names its content ([`handle`]), so one an
    /// earlier output issued is the same continuation: retaining it again
    /// refreshes its retention.
    pub fn retain(
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
        if let Some((_, owned)) = pending.evidence
            && pending
                .issued
                .values()
                .any(|member| member.evidence.as_ref() == Some(&owned.digest))
        {
            let key = evidence::key(
                &self.policy,
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
            let key = self.key(&token);
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
                access: pending.access,
                github_identity: pending.github_identity.clone(),
                member,
            };
            let bytes = octocode_engine::json_size::bytes(&issued).unwrap_or(usize::MAX);
            if !self
                .store
                .put_until(
                    crate::execution::cancel::deadline_of(context),
                    key,
                    issued,
                    bytes,
                    retention,
                )
                .1
            {
                return Err(ExecutionError::WorkerFailed);
            }
        }
        Ok(())
    }

    fn mutation_lock(
        &self,
        context: &ExecutionContext,
    ) -> Result<octocode_io::lock::Turn<'_>, ExecutionError> {
        self.mutation.take(
            crate::execution::cancel::deadline_of(context),
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
            .map_err(|_| ExecutionError::StorageUnavailable)?;
        let file = octocode_io::lock::open_lock_file(&disk.join("continuations.lock"))
            .map_err(|_| ExecutionError::StorageUnavailable)?;
        // One stable sidecar bounds lock files and serializes short merges.
        loop {
            context.check()?;
            let wait = crate::execution::cancel::deadline_of(context)
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            match octocode_io::lock::lock_within(
                file.try_clone()
                    .map_err(|_| ExecutionError::StorageUnavailable)?,
                wait,
                octocode_io::lock::LOCK_POLL,
            ) {
                Ok(lock) => return Ok(Some(lock)),
                Err(octocode_io::lock::LockError::Busy) => {}
                Err(octocode_io::lock::LockError::Io(_)) => {
                    return Err(ExecutionError::StorageUnavailable);
                }
            }
        }
    }

    /// The work `token` resumes for `tool` under `access`, or why it does
    /// not open here.
    pub fn open(&self, tool: ToolId, token: &str, access: [u8; 32]) -> Result<Opened, Refusal> {
        issued::parts(token).ok_or(Refusal::Malformed)?;
        let issued = self
            .store
            .get(&self.key(token))
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
    pub fn evidence(
        &self,
        opened: &Opened,
    ) -> Option<(String, Arc<octocode_github::ProviderEvidence>)> {
        let digest = opened.issued.member.evidence.as_deref()?;
        let evidence = evidence::open(
            &self.evidence,
            &self.held_evidence,
            &evidence::key(&self.policy, opened.github_identity(), digest),
        )?;
        Some((digest.to_owned(), evidence))
    }
}

impl Pending {
    /// The provider evidence `tool`'s own tool continuations issued by
    /// this output own ([`evidence::owned`]).
    pub fn own_evidence(&mut self, tool: ToolId, observed: octocode_github::ProviderEvidence) {
        self.evidence = evidence::owned(observed).map(|owned| (tool, owned));
        self.sealed = true;
    }

    /// `tool`'s own tool continuations issued by this output own the evidence
    /// already retained under `digest`.
    pub fn keep_evidence(&mut self, tool: ToolId, digest: &str) {
        self.evidence = Some((tool, evidence::Owned::retained(digest)));
        self.sealed = true;
    }

    pub fn owns_evidence(&self) -> bool {
        self.sealed
    }

    pub fn is_empty(&self) -> bool {
        self.issued.is_empty()
    }

    pub fn bind_github_identity(&mut self, identity: Option<String>) {
        self.github_identity = identity;
    }

    /// The handle of `work` resuming `tool` with `input`, issued once per
    /// output; the same continuation always has the same handle
    /// ([`handle`]).
    pub fn insert(&mut self, tool: ToolId, input: Value, work: SavedWork) -> String {
        let token = handle(&self.policy, &self.access, tool, &input, &work);
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

/// The opaque handle of one continuation: a digest of the replay policy,
/// access identity (the call's credential scope), tool, complete canonical
/// input and saved work (a response page's replay key). The policy binds
/// configuration, workspace, contract and planner epoch, so reissuing work
/// cannot resurrect an older policy's token. Within one policy the same
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
