Load when tracing a tool's inputs and data flow through the native runtime.

# Implementation audit

## Schema and implementation alignment

1. Start from `scripts/tool-inventory.mjs <tool>`.
   - Zero-hit field: read the Rust request struct (`#[serde(rename_all = "camelCase")]`, `rename`, `alias`) and prove whether it is read. Unread is dead input (remove from core) or a bug (wire it).
   - Unclassified field: the coverage gate is incomplete; classify it.
2. For each field labeled `consumed`/`forwarded`/`output-control`, confirm with `lspSearch` references that it changes behavior; deserialized and logged is not consumed.
3. Flag Rust validation duplicating core rules (`nativeRules`/`prepare.rs`) and hand-mirrored enums/strings; a new schema value must fail compilation or a test, not fall into `_ =>`.

## Data flow

Trace one real call per variant: `$OCTO <tool> '<json>'` → `contracts/prepare.rs` → `runtime/dispatch.rs` → `crates/tool-<family>/src/<tool>/` → provider/engine → `response/` → render. Per hop record what it adds, copies, discards.

- Redundant hops: re-parsing JSON, cloning payloads, double conversion, data the output drops.
- Provider/API: count requests per query; flag N+1 loops, missing batching (GraphQL), refetching data already in the response, unbounded fan-out.
- Errors: row-isolated, typed, actionable; no panic/unwrap on external input; no masking (`unwrap_or_default` on a failed fetch).

## Efficiency

- Quadratic scans, sort-then-filter, repeated regex compilation, whole-file reads for a window.
- Reads, pipes and child processes are bounded (OOM, zombie LSP servers, ReDoS via `regexEngine:"pcre2"`).
- Parallelism only where measured (`octocode-agentic` or release-profile timing).

## Caching

- What is cached (GitHub responses, clones, LSP sessions, clasify pages), key composition (ref/SHA, auth scope, query shape), TTL, invalidation, size cap.
- Flag a missing cache for an identical repeated provider call, a branch keyed without SHA, and keys or disk paths that mix tokens or users.
- Verify cold vs warm with two identical calls; regressions in `tests/tool_cache_contracts.rs`.

Record per finding: hop, file:line, reproducing call, measured cost if claimed, fix owner (core, native or engine).

Next: `references/output-audit.md`.
