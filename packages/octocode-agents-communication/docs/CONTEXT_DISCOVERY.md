# Scoped context without another mailbox

Use messages for questions, requests, decisions and handoffs. Use a short document
note for a reusable fact that a later collaborator should discover. A broadcast
only reaches its current recipient snapshot; it cannot serve future agents.

The implementation extends existing immutable documents and their `document.created`
audit metadata. It adds no table, migration, background model, embedding index,
injection hook or second delivery owner. CLI, bound MCP and Pi tools share the
same catalog and Rust implementation. Generic agents can use the CLI directly.

## Publish and discover

Using your existing workspace/database/session binding:

```sh
agents-communication share_document '{"name":"transport-gotcha-v1.md","reasoning":"Prevent premature handoff completion after transport submission","content":"Evidence: rust/dispatch.rs. Submission records transport acceptance; the recipient must finish required work before ACK.","context":{"summary":"Submission is not handling; wait for recipient ACK before treating a handoff as complete.","path":"rust","kind":"tree","branch":"main","ttlMs":86400000}}'
agents-communication context '{"path":"rust/transport.rs","branch":"main"}'
agents-communication read_document '{"name":"transport-gotcha-v1.md","limit":1024}'
```

- `summary`: at most 320 characters, one fact and its practical consequence.
- `path`: canonical workspace-contained path, default `.`; missing paths are valid.
- `kind`: `tree` by default; `file` matches that file only. Trees include descendants,
  not similarly named prefixes. Comparison uses the conservative lease namespace.
- `branch`: optional exact label supplied by the author and reader. The runtime does
  not invoke Git or guess the current branch. Branch-specific notes are excluded
  when the caller does not supply that branch.
- `ttlMs`: one second to seven days; default one day. Identical publication retries
  preserve the original expiry. Expiry hides discovery, never deletes evidence.

Discovery returns names, authors, short metadata and audit IDs, never document
bodies or agent histories. `read_document` checks the original content hash.
Summaries are author claims, not verified truth. Read relevant evidence, inspect
current code, and ask the author when notes conflict or appear stale. Changed
content or scope requires a new document name; there is no automatic supersession.

## Trigger and cursor contract

Look up context at a task or path boundary, after discovering peers and before
planning an overlapping change. Do not call it after every message/tool event.
Publication does not notify anyone. Send a targeted message when action is needed;
use passive topics only when current subscribers benefit from the new fact.

Each call examines at most 200 audit rows and returns at most 20 notes (default 10).
Follow `next` unchanged, including on empty pages, before claiming absence. A fresh
lookup freezes an audit-ID ceiling; concurrent publications appear on a later
lookup. Once complete, keep `cursor` as `after` for the **same path and branch**,
omitting `through`, to retrieve only later publications. Changing scope or losing
host context requires a fresh lookup. Cursors do not ACK messages or record that
an agent understood a note. Expiry is evaluated at each page's read time.

This scan favors bounded work without a migration. Very large audit histories may
require many pages. Measure that workload before introducing a dedicated index;
do not hide incomplete coverage or automatically fan out historical notes.

## Alternatives challenged

| Option | Decision and tradeoff |
| --- | --- |
| Messages/notify-all only | Keep for active collaboration. Insufficient for late joiners and reusable facts. |
| New memory table plus injection hook | Defer. Adds lifecycle, invalidation and competing context-delivery behavior before proving a need. |
| Existing document metadata plus pull | Implemented. Reuses provenance, immutable evidence and generic CLI access. |
| Classify every incoming note | Do not enable. Exact path/branch filtering needs no model and classification adds latency and provider cost. |
| Optional semantic question over large evidence | Use Octocode `clasify` only when bounded metadata cannot locate the answer. It can screen supplied resources, not extract another agent's hidden memory. Verify the selected source; measure host tokens and provider cost separately. |

The architectural benefit is one authority for documents and audit. The product
benefit is discovery after the author leaves. The main risk is stale or conflicting
claims, so discovery is scoped, expiring and explicit rather than mandatory prompt
injection. This is a small extension, not a general agent-memory service.

The design follows [Anthropic's just-in-time retrieval guidance](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
and [guidance to return relevant tool context](https://www.anthropic.com/engineering/writing-tools-for-agents).
Those sources motivate the choice; the local measurements below establish its
observed behavior.

## Measured result

`node scripts/context-discovery-benchmark.mjs` compares paginated audit discovery
with scoped lookup over identical stored notes: 32 documents, 220 unrelated audit
events, six path/branch cases, three alternating-order repetitions. Every response
page counts. Expected relevant names and zero repeated notes are strict assertions.

On the September 26 macOS ARM64 release run, scoped discovery returned **98.5% fewer
bytes** at the paired median. Retrieval p95 was **27.2 ms**, versus **45.1 ms** for
audit discovery. All expected notes were found and incremental reads repeated none.
The frozen result is `.octocode/benchmarks/communication-context-discovery/retrieval/result.json`.

This is a deterministic retrieval replay, not a provider-token or coding-quality
benchmark. The earlier [eight context-profile trials](CONTEXT_PROFILES.md) still
missed their total-token and latency gates; this result does not overturn them.
