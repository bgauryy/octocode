# Three-agent collaboration review

September 24, 2026. Ratings are engineering judgments for cooperating agents on one
local machine, not benchmark scores or production certification.

**Overall: 7.5/10. The coordination runtime works; unattended research still needs
supervision.**

| Area | Rating | Evidence and remaining limit |
| --- | --- | --- |
| Path locks | 8/10 | Seven real conflicts; exclusive handoff Codex → Pi → Claude. Each published within its owned lease and released it. Codex and Claude renewed successfully. Pi skipped the requested renewal step, but its lease remained valid. No fairness queue or enforcement against arbitrary OS writes. |
| Messages | 8.5/10 | 36 stored messages, 54 acknowledged deliveries, direct questions/answers and topic fanout. Retrying START retained one message and three deliveries. Durable receipt and retry deduplication do not prove exactly-once effects. |
| Coordination | 7/10 | All three vendor CLIs used the copied skill, discovered peers, subscribed, answered questions, and negotiated handoffs. Codex and Pi finalized before the last peer finding. A prompt alone does not enforce a completion barrier. |
| Skill and CLI | 8.5/10 | Short skill, focused command help, bound tools, and one shared database worked across vendors. Packaging remains validated on macOS ARM64 only. |
| Autonomous research quality | 6/10 | Seven peer questions and eight replies produced useful evidence exchange, but unsupported claims propagated; five bodies violated the requested JSON format. One parent correction was necessary. |

## What actually ran

Three real processes: Codex `gpt-6-luna`, Claude `haiku`, and Pi
`guy-provider-anthropic-x/claude-haiku-4-5-20251001`. Each received the embedded
communication skill and a distinct source packet: locks, messages, or coordination.
The parent fetched local source with Octocode and supplied verified primary-source
summaries. These were source-packet researchers, not three independent web browsers.

The workers subscribed to one shared topic. The controller initially held a tree
lease on `research`; all three were denied its child `research/report.md`. After
release, they requested handoffs and took exclusive turns publishing findings.
The lease guarded publication, not an actual filesystem edit. All three conclusions
arrived about 146 seconds after the first vendor-ready event. Every delivery was
acknowledged; no lease, live test session, or owned vendor process remained.

The separate deterministic suite previously passed 10 Rust and 16 CLI/process tests,
including alias paths, expiry, stale IDs, concurrent acquisition, and Python-only
SQLite interoperability. This review did not rerun that unchanged suite.

## What failed and why it matters

1. **Evidence discipline:** Pi inferred non-atomic fanout from an incomplete source
   packet. The parent supplied `rust/store.rs:313-368` and
   `rust/database.rs:63-67`; Claude confirmed the transaction and Pi corrected the
   claim. The actual implementation inserts the message and all recipient rows in
   one transaction. A schema alone cannot establish transaction boundaries.
2. **Completion discipline:** Codex and Pi sent final conclusions before Claude's
   finding. A host must check required peer results before declaring collaboration
   complete; model agreement or a `final` label is insufficient.
3. **Structured output:** Bodies 25, 31, 33, 34 and 36 contained malformed JSON.
   Message bodies are opaque strings in the product; transport preserved them.
   The strict test harness could not parse completion and was stopped by the
   supervisor after the real work and acknowledgements completed. This is recorded
   as a failed autonomous workflow, not a fully green end-to-end run.
4. **Semantic overclaims:** Sender retry keys deduplicate insertion, not downstream
   effects. ID-ordered reads do not guarantee global processing order. Resuming a
   communication identity starts fresh vendor history; Pi's earlier reattachment
   claim was unsupported. The final ratings exclude these incorrect claims.

## Practical next changes

- Keep free-text messages for conversation. If a host needs machine-readable
  workflow state, use a small validated envelope or validate before accepting a
  result; avoid requiring models to hand-escape JSON inside message strings.
- Enforce completion barriers in the host, with expected contributors and observed
  results. Keep progress messages short and use direct questions for missing facts.
- Teach the workflow to mark missing evidence as unknown and verify peer claims
  against exact code. Do not turn every research opinion into another broadcast.
- Keep cooperative lease semantics explicit. Add a wait/owner-release mechanism
  only if real contention warrants it. Strict write protection would need a write
  destination that rejects stale ownership, not merely another lease counter.

SQLite documents the single-writer transaction model and same-host WAL constraint:
[transactions](https://www.sqlite.org/lang_transaction.html),
[WAL](https://www.sqlite.org/wal.html). Expiry alone cannot fence stale writes;
[fencing requires destination enforcement](https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html).
Applications must handle repeated effects explicitly; see the
[idempotent-consumer guidance](https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/standard-queues-at-least-once-delivery.html).
These sources inform the design assessment, not claims of untested guarantees.

## Evidence

- `out/research-collaboration.json`: independent tool/database verification, metrics,
  lease timelines, raw findings/conclusions, snapshot hashes, and explicit failed
  workflow checks (`runtimePassed: true`, `autonomousResearchPassed: false`).
- `out/research-collaboration-raw.json`: original trace and messages, including the
  strict harness shutdown failure. Original malformed bodies are retained.
- `.octocode/tmp/communication-research/`: research packets, harness and verifier
  under the repository root. The verifier uses manually reviewed type tags for
  this run's malformed bodies; it does not repair them or claim generic parsing.

No runtime implementation or Awareness behavior changed in this review.
