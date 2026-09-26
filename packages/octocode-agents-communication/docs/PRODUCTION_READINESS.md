# Production readiness

Review date: 2026-09-26. Scope: a local, same-user communication service for
cooperating agents. This is not a multi-tenant broker or a filesystem security
boundary. The package remains private and unpublished.

Subsequent audit-intent change: document publication now requires `reasoning` in
the shared CLI/MCP/Pi contract and retains it in the existing audit metadata. The
skill also requires a purpose for every file/path audit. That revision used 39
lines and 7,505 bytes; subsequent wording cleanup retains the workflow with fewer bytes.
This change adds no delivery layer or read-audit stream; the live six-agent results
below belong to the preceding frozen runtime, not this updated artifact.
The updated artifact passes all 13 document tests, 33 Rust tests, formatting and
Clippy. The full 269-test process run had 259 passes, eight SQL-environment skips
and two macOS artifact-startup timeouts; the SQL checks and all five artifact tests
passed separately on recheck. This does not resolve the intermittent loader issue.

September 26 handling repair: two Claude, two Codex and two Grok agents passed
both the review and handoff scenarios on the same runtime. Each run completed
30 agent-authored requests, 30 correlated replies, all 30 contributor-document
reads, six broadcast recipients and zero pending messages. Routing used no model
and neither run needed host prompts after initialization. All child processes exited.

The fix aligns Codex initialization with the other hosts, names the actual tools
in the task, and puts ACK-only completion in the triggering message. The skill
asks senders to state completion criteria and recipients to check each delivered
ID against successful tool results. No automatic ACK, replay or new retry loop
was added. That run used the 39-line, 7,366-byte skill.

The document grader also had a defect: it demanded an old filename even when a
peer requested and successfully read a corrected immutable revision. Verification
now follows each request's document name and checks publisher, hash and successful
read. Four new regression tests reject old/missing reads, wrong hashes/publishers,
ambiguous references and failed MCP calls. The original 25/30 trial and subsequent
failed candidates remain preserved; they were not relabeled as complete passes.

Evidence: `.octocode/benchmarks/communication-handling-fix/verification.json` and
its frozen runtime, harnesses, native events and SQLite snapshots. The final
runtime passes the 263-test process/integration suite and 33 Rust tests; the
updated harness passes its 12 focused checks. The extracted-bundle check first
hit the known macOS startup timeout and passed on recheck. That distribution
issue remains open. These small samples establish the tested flows, not universal
model compliance or token savings. Earlier [context-cost gates](CONTEXT_PROFILES.md)
remain unmet; [scoped discovery](CONTEXT_DISCOVERY.md) measures retrieval only.

## Decision

**Controlled local pilot; not a general production release.** The DB-backed
communication protocol has substantial automated and live-host evidence. Universal
unattended editing and distribution across supported platform selectors have not
met release gates. A passing messaging matrix does not prove either property.

## Repairs in this review

- A larger live matrix exposed a 16-message staging / four-receipt confirmation
  mismatch that stalled Pi. Staging now derives its cap from the receipt catalog;
  Pi chunks accumulated confirmations using the same limit. Regression coverage
  includes 35 buffered messages, durable flush, continued delivery and no replay.
- The delivery prompt explicitly requires tool ACKs for handled answers/FYIs;
  prose acknowledgements do not mutate the DB. This improves the instruction,
  not the runtime's ability to guarantee model compliance.

- Native delivery rechecks attachment identity inside staging; attachment changes
  cannot bypass an in-flight attempt through heartbeat/entity updates.
- Codex socket I/O has an absolute deadline, including fragmented frames that keep
  each individual read alive. Workspace identity is validated before staging.
- Build and pack validate startup and extracted contents before publication;
  unchanged rebuilds preserve the installed inode. Failed validation preserves the
  previous artifact. These checks contain failures; they do not fix the OS loader.

- Explicit `send_message` completion: `replyTo` plus `ackReply:true` stores a final
  direct reply and acknowledges the incoming request in one transaction. Failed
  sends cannot acknowledge it; identical retries preserve message and ACK identity.
  Clarification/partial replies remain pending. CLI, MCP and Python share the rule.
- `check_write` checks the bound identity's own live lease coverage for concrete
  file paths, including new files. The check acquires and renews nothing. It is a
  host integration point, not another model tool or an OS fence. Pi, Claude and OpenCode offer
  opt-in structured write/edit guards; see [coverage and setup](HOST_LEASE_GUARDS.md).

## Release gates

| Gate | Evidence / remaining requirement |
| --- | --- |
| Durable messages and audit | Versioned SQLite schema, immutable message keys, atomic writes, bounded reads, append-only audit; regression suite and recovery soak |
| No routine duplicate context | Stage before I/O; ambiguous attempts require explicit inspected retry. Exactly-once external effects are not promised |
| Native delivery | Live two-recipient tests for Claude, Codex, Grok, Pi and OpenCode; raw interoperability. Host versions and receipt strength remain part of the deployment contract |
| Write coordination | Advisory leases recover crashed owners. Earlier real editing trials created an unleased test file. A structured-tool guard cannot cover arbitrary shell writes or enforce OS fencing |
| Startup and distribution | Fresh macOS executable copies intermittently stalled before Rust main. Bounded validation can reject a stalled artifact, but root cause remains unresolved; see [startup evidence](MACOS_STARTUP.md) |
| Platform coverage | macOS ARM64 measured. Six-target native CI and receipt verification are implemented; hosted runs remain required. See [release gates](RELEASE_CHECKLIST.md) |
| Public release trust | Extracted-archive upgrade/backup/restore smoke covers v1–v5 to v6 locally. Publisher signing/notarization and authenticated artifact provenance still need a release owner |
| Host compatibility | Grok leader IPC and Codex experimental APIs are version-sensitive. Cursor has fixture coverage, not a live validation claim. Raw fallback is explicit and cannot wake arbitrary hosts |
| Operations | Read-only paginated `health` exposes stalled, uncertain, expired and offline action deliveries without replaying context. Supervision, storage monitoring and DB plus document backups remain operator duties. See [operations](OPERATIONS.md) |

## Pilot deployment

1. Use the tested platform and pin the actual host versions. Keep all identities on
   one canonical workspace and DB. Run under the same trusted OS user.
2. Install the skill once, bind one delivery owner per identity, expose the needed
   tools, and verify a request, correlated reply and handling ACK before work.
3. Supervise listeners and alert on exit. Heartbeats maintain presence; they do not
   renew leases. Stop editing on expiry or a failed lease renewal.
4. Keep editing supervised unless every mutating surface is guarded by the host.
   Structured-tool checks are useful defense in depth; shell access remains outside
   that guarantee. Do not confuse a model instruction with enforced ownership.
5. Export a verified DB snapshot and preserve `.octocode/communication/` separately.
   Stop workers before migration/restore. Inspect uncertain attempts before retry;
   never restart a failed mutation blindly.

## Evidence and ratings

The [benchmark record](BENCHMARKS.md) retains previous successes and failures;
[context evaluation](CONTEXT_OPTIMIZATION.md) records strict editing failures.
Ratings are engineering judgments about this scope, not probabilities of success.

| Layer | Rating | Reason |
| --- | --- | --- |
| DB and audit | 9/10 | Atomic transitions, raw-client parity, verified export, retention without deleting history |
| Messaging and native adapters | 8/10 | No routing model, explicit receipt/ACK distinction, guarded binding and deadlines; private API/version exposure remains |
| Leases and edit coordination | 8/10 | Atomic leases, crash recovery and native-bound structured guards for Pi/Claude/OpenCode; ambiguous host paths fail closed; arbitrary shell writes remain advisory |
| Hooks and fallback | 8/10 | One delivery owner, idle fast path, generic CLI/SQL; no universal idle wake |
| Instructions and context | 7/10 | One short skill, selected tools, scoped document discovery, atomic completion; model response compliance and total-token savings are not guaranteed |
| Release and operations | 7/10 | Read-only operational diagnostics, extracted upgrade/restore checks and strict six-target CI receipts; loader, signing and unexecuted platform gates remain |

Overall: **8/10 for the controlled local communication scope; 7/10 for a general
production release.** These are qualitative readiness scores, not averaged test
results or reliability percentages.

## Validation record

Previous frozen messaging-matrix executable SHA-256:
`79faf0d756af82c93c01a7efb804dd4c407ae69cdbf9213acb4e6dd8a21ce385`.
That frozen release passed 213 process/integration tests (zero failures/skips), 31
Rust tests, strict Clippy, formatting, docs links and the standard skill validator.
RustSec checked 189 dependencies with zero known vulnerabilities or warnings.
The extracted archive passed checksum, signature integrity, startup, embedded-skill
and launcher checks. The signature is ad-hoc integrity, not publisher identity.

A new 30-cycle model-free recovery run passed: 36 messages acknowledged, zero
pending messages, unexpected duplicate offers or leaked children. Its 12 explicit
retries produced 48 external offers; those requested retries are not counted as
unexpected duplication.

An alternating 30-pair CLI benchmark measured final reply plus ACK at p50
12.99 ms / p95 14.07 ms versus separate reply and ACK at p50 25.73 ms / p95
26.69 ms. It reduces two CLI processes/tool calls to one. It excludes inference
and establishes no provider token or cache-hit savings. That skill was 6,802 bytes;
four selected communication descriptors are 4,340 bytes versus 12,979 for all 13.

Evidence: `.octocode/benchmarks/communication-production/verification.json`,
`completion.json`, and `final-integration-tests.log` in the repository. Earlier
failed and interrupted runs are retained rather than overwritten.

The live run at `2026-09-25T18-13-36.736Z` failed its strict single-answer gate:
OpenCode-2 sent `ANSER` under `answer-20`, then a corrected answer under
`answer-20-fix`. This was a second explicit send, not dispatch replay. That run
used the immediately preceding executable `e9e9da99…`; the final artifact adds
first-attachment protection during staged delivery, covered by a regression.
The harness now continues broadcast diagnostics after reply-quality deviations,
while retaining a failing final verdict for extra, missing or malformed answers.


The frozen-artifact live run at `2026-09-25T18-20-05.459Z` passed: two each
of Claude, Codex, Grok, Pi and OpenCode plus one raw peer exchanged 110 questions
and 110 correlated answers; all 242 deliveries were acknowledged, all 11 peers
received the broadcast, all ten native peers read the document, and no duplicate
reply, pending message or owned child remained. Routing used zero model calls and
no host prompts were sent after startup. The controller is a separate twelfth DB
identity. The response-quality failure above remains evidence of model variability.

That run recorded 240 recipient tool calls versus 344 in the prior passing
run, consistent with combining final reply and ACK. This is an observed single-run
comparison, not a provider token/cache or statistical latency claim. Usage records
retain their request/turn/cumulative scopes and must not be blindly summed.

## Follow-up hardening validation

Hardening-test executable SHA-256:
`46c646562aa9bd838a903d80151aff2812c379e89100be3fc57edafbd764fbab`.
It passed **238 process/integration tests**, zero failures/skips, in 98.55 seconds;
31 Rust tests, strict Clippy, formatting, documentation links and skill validation
also passed. The skill remains 37 lines (7,005 bytes). The earlier cross-vendor
messaging matrix above is bound to its own artifact; it was not rerun for these
operational diagnostics and host-admission changes.

New coverage includes read-only paginated `health`, native identity plus lease
coverage in one snapshot, Pi path-alias mismatch rejection, Claude/OpenCode
structured guards, and release-receipt integrity. Host trials and their exact
versions/artifact boundaries are documented in [host lease guards](HOST_LEASE_GUARDS.md).
OpenCode's real free-provider trial returned 403; installed-host execution passed
with a deterministic local provider. Those are distinct evidence categories.

The extracted macOS ARM64 archive passed CLI/MCP message/reply/ACK, lease handoff,
and all v1–v5 upgrade → export → restore → resumed protocol checks. Archive SHA-256:
`e6e65384488ceb15c70790652d350b54f30036919b0fa402c89e22c69666ad3c`.
The strict release verifier rejected this dirty local source as designed; no
commit or publication was performed. Six-platform CI is authored, not executed.
Evidence: `.octocode/benchmarks/communication-production-readiness/final-hardening/`.

A matched 20-start signing experiment found no reproduced stall and no benefit
from explicit ad-hoc re-signing. It does not invalidate the previous loader stalls.
No valid publisher signing identity is installed. **A 10/10 production claim is
not supported** until the remaining platform, publisher-trust and startup gates
have evidence; bypassing those gates would reduce quality.
