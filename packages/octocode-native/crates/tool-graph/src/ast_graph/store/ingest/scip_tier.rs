//! The opt-in SCIP tier of a build: with the user's consent
//! ([`octocode_engine::lsp::config::scip_consent`]) one `rust-analyzer scip`
//! run per cargo workspace and input state feeds the call linker a precise
//! index. Any failure leaves the build with its syntax-only edges.
use super::*;
use crate::ast_graph::store::scip::{ScipIndex, ScipInput};
use octocode_engine::lsp::scip_run::{ScipError, ScipPlan, scip_plan};

enum Tier {
    /// No consent, or no cargo workspace: nothing is spawned.
    Off,
    /// Consented: the index is read (and run, when not cached) only once the
    /// build holds Rust files.
    Planned(Box<ScipPlan>),
    /// Consented, but the plan failed (no answer to `--version`, unreadable
    /// inputs, a stopped caller).
    Unavailable,
}

pub(super) struct Scip {
    tier: Tier,
    status: ScipStatus,
    /// Why the tier failed, for the build's warning.
    failure: Option<String>,
}

/// The identity a build records when its indexer failed: unequal to the
/// planned key, so the next build runs the indexer again.
const FAILED_IDENTITY: &str = "failed";
const UNAVAILABLE_IDENTITY: &str = "unavailable";

impl Scip {
    /// No SCIP tier.
    #[cfg(test)]
    pub(super) fn off() -> Self {
        Self {
            tier: Tier::Off,
            status: ScipStatus::Off,
            failure: None,
        }
    }

    /// The plan for the scope `options` names: spawns nothing without the
    /// user's consent.
    pub(super) fn plan(options: &IngestOptions) -> Self {
        let mut failure = None;
        let tier = match options.lsp.as_ref() {
            None => Tier::Off,
            Some(lsp) => match scip_plan(&options.path, lsp) {
                Ok(plan) => Tier::Planned(Box::new(plan)),
                Err(ScipError::NotConsented | ScipError::NoCargoWorkspace) => Tier::Off,
                Err(error) => {
                    failure = Some(error.to_string());
                    Tier::Unavailable
                }
            },
        };
        Self {
            tier,
            status: ScipStatus::Off,
            failure,
        }
    }

    /// The tier's part of the build identity: `None` when off, else the cache
    /// key (so toggling consent or changing any input builds again).
    pub(super) fn identity(&self) -> Option<String> {
        match &self.tier {
            Tier::Off => None,
            Tier::Planned(plan) => Some(plan.key.clone()),
            Tier::Unavailable => Some(UNAVAILABLE_IDENTITY.to_owned()),
        }
    }

    /// What a snapshot built under [`Self::identity`] records after the run:
    /// a failed run records another identity, so a retry is not answered by
    /// the syntax-only snapshot.
    pub(super) fn recorded_identity(&self) -> Option<String> {
        match (&self.tier, self.status) {
            (Tier::Planned(_), ScipStatus::Failed) => Some(FAILED_IDENTITY.to_owned()),
            _ => self.identity(),
        }
    }

    /// The status a reused snapshot answers with: it was built under this
    /// identity, so an index fed it.
    pub(super) fn reused(&self) -> ScipStatus {
        match self.tier {
            Tier::Off => ScipStatus::Off,
            Tier::Planned(_) => ScipStatus::Indexed,
            Tier::Unavailable => ScipStatus::Failed,
        }
    }

    pub(super) fn status(&self) -> ScipStatus {
        self.status
    }

    /// Why the tier failed, when it did.
    pub(super) fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// The index for `built`, when consented and the build holds Rust files:
    /// cached under `<home>/scip/<root tag>` or run once. Records the status.
    pub(super) fn load(
        &mut self,
        home: &Path,
        built: &super::super::super::types::BuiltGraph,
    ) -> Option<ScipIndex> {
        let plan = match &self.tier {
            Tier::Off => return None,
            Tier::Unavailable => {
                self.status = ScipStatus::Failed;
                return None;
            }
            Tier::Planned(plan) => plan,
        };
        if !built.facts.keys().any(|file| file.ends_with(".rs")) {
            return None;
        }
        let cache = home
            .join("scip")
            .join(super::super::snapshots::root_tag(&plan.cargo_root));
        let index = plan
            .index(&cache)
            .map_err(|error| error.to_string())
            .and_then(|file| {
                octocode_tools_shared::source::read_bounded(
                    &file.path,
                    crate::ast_graph::limits::LIMITS.snapshot_bytes,
                )
                .map_err(|error| format!("{error:?}"))
            })
            .and_then(|bytes| {
                let prefix = scope_prefix(&plan.cargo_root, &built.root)?;
                let bytes = rebase(&bytes, &prefix)?;
                ScipIndex::parse(&bytes).map_err(|error| format!("{error:?}"))
            });
        match index {
            Ok(index) => {
                self.status = ScipStatus::Indexed;
                Some(index)
            }
            Err(error) => {
                self.status = ScipStatus::Failed;
                self.failure = Some(error);
                None
            }
        }
    }
}

/// The projection's input for `index`: each graph file's text read from the
/// build's root.
pub(super) fn input<'a>(
    index: &'a mut ScipIndex,
    text: &'a dyn Fn(&str) -> Option<String>,
) -> ScipInput<'a> {
    ScipInput { index, text }
}

/// The scope's path under the SCIP root, `/`-separated (`""`: the same
/// directory).
fn scope_prefix(scip_root: &Path, scope: &Path) -> Result<String, String> {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (root, scope) = (canonical(scip_root), canonical(scope));
    let within = scope
        .strip_prefix(&root)
        .map_err(|_| "the graph scope is outside the cargo workspace".to_owned())?;
    Ok(within
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn varint(bytes: &[u8], at: &mut usize) -> Result<u64, String> {
    let mut value = 0_u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*at).ok_or("truncated SCIP index")?;
        *at += 1;
        value |= u64::from(byte & 0x7f)
            .checked_shl(shift)
            .ok_or("bad varint")?;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("bad varint".into())
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(u8::try_from(value & 0x7f).unwrap_or_default() | 0x80);
        value >>= 7;
    }
    out.push(u8::try_from(value).unwrap_or_default());
}

/// The fields of one protobuf message: `(field, wire type, raw bytes of the
/// value (the payload for a length-delimited one))`.
type Field<'a> = (u64, u64, &'a [u8]);

fn fields(bytes: &[u8]) -> Result<Vec<Field<'_>>, String> {
    let (mut at, mut out) = (0, Vec::new());
    while at < bytes.len() {
        let tag = varint(bytes, &mut at)?;
        let (field, wire) = (tag >> 3, tag & 7);
        let start = at;
        let end = match wire {
            0 => {
                varint(bytes, &mut at)?;
                at
            }
            1 => at + 8,
            5 => at + 4,
            2 => {
                let len = usize::try_from(varint(bytes, &mut at)?).map_err(|e| e.to_string())?;
                at += len;
                at
            }
            _ => return Err("unsupported wire type".into()),
        };
        let value = bytes.get(start..end).ok_or("truncated SCIP index")?;
        out.push((field, wire, value));
        at = end;
    }
    Ok(out)
}

fn put_field(out: &mut Vec<u8>, field: u64, wire: u64, value: &[u8]) {
    put_varint(out, field << 3 | wire);
    out.extend_from_slice(value);
}

/// `index` with each document's `relative_path` made relative to the graph
/// scope `prefix` names under the SCIP root, and the documents outside the
/// scope dropped: SCIP paths are relative to the root `rust-analyzer scip`
/// ran on, graph file keys to the scope. `prefix` empty: unchanged.
fn rebase(index: &[u8], prefix: &str) -> Result<Vec<u8>, String> {
    if prefix.is_empty() {
        return Ok(index.to_vec());
    }
    let scope = format!("{prefix}/");
    let mut out = Vec::with_capacity(index.len());
    for (field, wire, value) in fields(index)? {
        if field != 2 || wire != 2 {
            put_field(&mut out, field, wire, value);
            continue;
        }
        // `value` holds the length prefix and the document message.
        let mut at = 0;
        let len = usize::try_from(varint(value, &mut at)?).map_err(|e| e.to_string())?;
        let document = value.get(at..at + len).ok_or("truncated SCIP index")?;
        let mut rebased = Vec::with_capacity(document.len());
        let mut inside = true;
        for (doc_field, doc_wire, doc_value) in fields(document)? {
            if doc_field == 1 && doc_wire == 2 {
                let mut path_at = 0;
                let path_len =
                    usize::try_from(varint(doc_value, &mut path_at)?).map_err(|e| e.to_string())?;
                let path = doc_value
                    .get(path_at..path_at + path_len)
                    .ok_or("truncated SCIP index")?;
                match std::str::from_utf8(path)
                    .ok()
                    .and_then(|path| path.strip_prefix(&scope))
                {
                    Some(relative) => {
                        put_varint(&mut rebased, doc_field << 3 | doc_wire);
                        put_varint(&mut rebased, relative.len() as u64);
                        rebased.extend_from_slice(relative.as_bytes());
                    }
                    None => inside = false,
                }
            } else {
                put_field(&mut rebased, doc_field, doc_wire, doc_value);
            }
        }
        if inside {
            put_varint(&mut out, field << 3 | wire);
            put_varint(&mut out, rebased.len() as u64);
            out.extend_from_slice(&rebased);
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(super) mod rebase_tests;
