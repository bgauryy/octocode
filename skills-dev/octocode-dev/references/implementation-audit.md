# Implementation audit

Load when tracing a tool's inputs and data flow through the native runtime. Why: the schema is a promise; this lane proves the code keeps it without waste.

## Schema ↔ implementation alignment

1. Start from `scripts/tool-inventory.mjs <tool>`.
   - zero-hit field → read the Rust request struct (`#[serde(rename_all = "camelCase")]`, `rename`, `alias`) and prove whether the field is read. Unread = dead input (remove from core) or bug (wire it).
   - unclassified field → the coverage gate is incomplete; classify it.
2. For each field labeled `consumed`/`forwarded`/`output-control`, confirm with `lspSearch` references on the struct field that it changes behavior; a field only deserialized and logged is not consumed.
3. Flag Rust-side validation that duplicates core rules (`nativeRules`/`prepare.rs`) — one owner.
4. Flag Rust enums/strings that mirror schema enums by hand; a new schema value must fail compilation or a test, not fall into `_ =>`.

## Data flow

Trace one real call per variant: `$OCTO <tool> '<json>'` → `contracts/prepare.rs` → `runtime/dispatch.rs` → `tools/<tool>/` → provider/engine → `response/` → render. Record for each hop: what it adds, what it copies, what it discards.

- Redundant hops: re-parsing JSON, cloning whole payloads, converting shapes twice, building data the output drops.
- Provider/API: count requests per query (`debug: true` or tests); flag N+1 loops, missing batching (GraphQL), refetch of data already in the response, and unbounded fan-out.
- Errors: row-isolated, typed, actionable; no panic/unwrap on external input; no masking (`unwrap_or_default` on a failed fetch).

## Efficiency and algorithms

- Complexity on real corpora: quadratic scans over files/matches, sort-then-filter where filter-then-sort works, repeated regex compilation, reading whole files for a window.
- Streaming and bounds: reads, pipes, and child processes are bounded (OOM, zombie LSP servers, ReDoS via `regex_worker`).
- Parallelism only where measured; claims need `octocode-eval-benchmark` or a release-profile timing.

## Caching

- What is cached (GitHub responses, clones, LSP sessions, clasify pages), key composition (ref/SHA, auth scope, query shape), TTL, invalidation, and size cap.
- Missing cache where an identical provider call repeats inside one session; stale cache where a mutable ref (branch) is keyed without SHA.
- Security: cache keys and disk paths never mix tokens or cross-user data.
- Verify cold vs warm with two identical calls; `tests/tool_cache_contracts.rs` for regressions.

## Record

Per finding: hop, file:line, reproducing call, measured cost if claimed, fix owner (core vs native vs engine).
