//! What a graph build holds per kept file, the bound on its peak, and what
//! a reader charges to admit a source table's origin back.

use octocode_engine::num::saturating_usize;

/// The bytes one graph build may hold at its peak: half of the memory this
/// process may use ([`octocode_engine::host::memory_mb`], the host's physical
/// memory or a container's smaller limit), the rest left to the host and
/// its other programs. A host whose memory cannot be read is taken as
/// [`UNKNOWN_HOST_MB`]. Linking needs every kept file until projection, so
/// a scope past this refuses (`scopeTooBroad`) rather than swapping; this is
/// the one place the rule is set.
pub(crate) fn build_budget() -> usize {
    build_budget_for(octocode_engine::host::memory_mb())
}

/// The memory, in MiB, a host whose memory cannot be read is taken to have.
pub(crate) const UNKNOWN_HOST_MB: u64 = 8 * 1024;

/// [`build_budget`] of a host with `host_mb` MiB.
pub(crate) fn build_budget_for(host_mb: Option<u64>) -> usize {
    let bytes = host_mb.unwrap_or(UNKNOWN_HOST_MB).saturating_mul(1 << 20) / 2;
    saturating_usize(bytes)
}

/// What keeping one file costs a build at its peak, beside its packed
/// source correspondence (charged as held): [`FACT_BUILD_QUARTERS`]
/// quarters of a byte per encoded byte of its facts (its checkpoint
/// artifact, else their JSON, which the artifact encodes) and
/// [`DECLARATION_BUILD_BYTES`] per declaration. Together they cover the
/// facts kept for linking, their linked copy and the rows and strings they
/// project, until projection releases them. Measured 2026-11-06 on release
/// builds as the peak RSS of fresh builds (`/usr/bin/time -l`) against each
/// scope's summed artifact bytes and declarations, least-squares fit
/// `peak = 1.25·artifact + 851·declarations + source + 207 MB` (declarations
/// rounded up to 1 KiB): prometheus (1,004 files, 339 MB of artifacts,
/// 16,809 declarations: 0.74 GB peak), kubernetes `pkg` (3,293, 931 MB,
/// 47,412: 1.48 GB), rustc `compiler` (2,214, 672 MB, 50,136: 1.01 GB),
/// vscode `src` (9,639, 3.12 GB, 247,909: 4.47 GB) and linux `drivers/gpu`
/// (7,731, 2.89 GB, 1,245,777: 4.76–5.32 GB), each within 11% of its fit.
/// The fixed 207 MB is the process's, not the build's, and is left out.
pub(crate) const FACT_BUILD_QUARTERS: usize = 5;
/// See [`FACT_BUILD_QUARTERS`].
pub(crate) const DECLARATION_BUILD_BYTES: usize = 1024;

/// The build charge of one kept file whose facts encode to `fact_bytes`,
/// holding `declarations` and `source_bytes` of packed source
/// correspondence ([`FACT_BUILD_QUARTERS`]).
pub(crate) fn build_charge(fact_bytes: usize, declarations: usize, source_bytes: usize) -> usize {
    (fact_bytes / 4)
        .saturating_mul(FACT_BUILD_QUARTERS)
        .saturating_add(declarations.saturating_mul(DECLARATION_BUILD_BYTES))
        .saturating_add(source_bytes)
}

/// Bytes of one origin's entry in the source table's index (SRCE); the
/// table asserts its entry has this size.
pub(crate) const ORIGIN_INDEX_BYTES: usize = 24;

/// What admitting `origin` back from a published source table charges:
/// its index entry plus its decode (unit id, every vector, every raw
/// source id), every item and every byte but its encoded record; the
/// table's tests price its index entries against it.
#[cfg(test)]
pub(crate) fn read_charge(
    origin: &octocode_engine::text::analysis::OriginProjection,
) -> (usize, usize) {
    use octocode_engine::text::analysis::{
        Contribution, OriginBranch, OriginSegment, OriginalRange,
    };
    use std::mem::size_of;
    let mut bytes = ORIGIN_INDEX_BYTES
        + origin.analysis_unit_id.len()
        + origin.segments.len() * size_of::<OriginSegment>();
    let mut items = 2 + origin.segments.len();
    let raw = |value: &OriginalRange, bytes: &mut usize, items: &mut usize| {
        *bytes += value.source_id.len();
        *items += 1;
    };
    for segment in &origin.segments {
        bytes += segment.branches.len() * size_of::<OriginBranch>();
        items += segment.branches.len();
        for branch in &segment.branches {
            match branch {
                OriginBranch::Copied(value) | OriginBranch::Transcoded(value) => {
                    raw(value, &mut bytes, &mut items);
                }
                OriginBranch::Derived(values) => {
                    bytes += values.len() * size_of::<Contribution>();
                    items += values.len();
                    for value in values {
                        if let Contribution::Source(value) = value {
                            raw(value, &mut bytes, &mut items);
                        }
                    }
                }
                OriginBranch::Unmapped(_) => {}
            }
        }
    }
    (bytes, items)
}
