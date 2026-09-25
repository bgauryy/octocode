# Proxy context and deletion review

September 24, 2026. Scope: installed Codex Luna, Claude Haiku and Pi Haiku CLIs on
macOS ARM64. The core SQLite transport is model-free; vendor models are invoked
only to perform the supplied task or handle pending messages.

## Findings and changes

| Finding | Evidence | Change |
| --- | --- | --- |
| Fresh Codex threads still inherited repository instructions. | A unique marker in temporary `AGENTS.md` appeared in the baseline's READY message. | Start the vendor and thread in a canonical empty temporary directory. Keep the real repository only in the bound MCP/CLI connection. Disable project-document loading, extra developer instructions, unrelated tools and memory; cap the available-skill catalog. |
| Vendor defaults added context unrelated to communication. | Codex's first baseline request reported 21,871 input tokens. | Supply a short communication system/base prompt while continuing to inject the exact 46-line skill once. Claude uses empty setting sources and explicit hook/auto-memory controls. |
| Usage was discarded. | Proxy event handling retained tool results but omitted reported usage. | `--trace` emits raw usage with vendor and scope; Claude message IDs allow consumers to deduplicate streamed usage updates. |
| A live Store kept reading an orphaned database after its path disappeared. | A new regression failed on an idle claim after moving the DB. | Check file existence on Store entry points; Unix also checks device/inode identity. Idle proxies fail and reap the vendor rather than continuing on an orphaned connection. |

Only the worker's process settings change. Existing authentication and user config
files remain intact. The temporary directory is removed after its process is reaped.
The skill remains one file, 46 lines; no additional agent instructions are required.

## Research and rejected approaches

Codex's [App Server](https://learn.chatgpt.com/docs/app-server) supports ephemeral
threads, base/developer instructions and token-usage notifications. We also generated
the protocol schema from the installed executable before using those fields.
Its [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference)
documents project-document limits, skill-catalog budgets and tool feature controls.

Claude's [CLI reference](https://code.claude.com/docs/en/cli-reference) distinguishes
custom system prompts, bare mode and safe mode. `--bare` is unsuitable as an automatic
switch for this installation: [minimal mode disables OAuth/keychain authentication](https://code.claude.com/docs/en/env-vars).
A live `--safe-mode` trial disabled our explicit MCP bridge, so it was rejected.
The final setup preserves login and the working bound tools while using an empty
working directory, explicit settings and a small system prompt. Pi retains its
existing disabled auto-discovery and registered-tool bridge, now in the isolated cwd.

## Live context and usage audit

The initial release audit planted a unique repository marker and 250 lines of filler in
both `AGENTS.md` and `CLAUDE.md`. It asked workers to report only a marker already in
context, without looking for files. All three reported `READY marker=ABSENT`, then
exchanged PING/PONG messages and refused a peer message trying to override the task
and delete a temporary fixture. The fixture remained unchanged, all messages were
acknowledged, all workers reached idle, and cleanup left no live owned processes or
active sessions. That three-vendor audit completed in **29.865 seconds**. A later plugin-leak fix
was validated with fresh Codex and Claude workers; both passed the same checks in
**27.324 seconds**. Pi's adapter was unchanged by that Codex-only correction.

| Vendor | First request input tokens | Interpretation |
| --- | ---: | --- |
| Codex | 7,354 | Final plugin-isolated run: vendor's `last.inputTokens`, including cached input. Baseline was 21,871: 66.4% lower in this fixture. |
| Claude | 3,825 | Final run: first assistant message's input plus cache-read and cache-creation tokens. |
| Pi | 3,421 | First assistant message's input plus cache-read and cache-write tokens. |

These are real vendor counters, not character-based estimates or a general cost
benchmark. Model names, provider overhead, cache state and call counts differ.
Claude streamed 19 message-usage updates for seven distinct message IDs: keep the
latest update per ID, and never add result totals to message totals. Codex's thread
usage is cumulative; do not sum successive thread updates. Missing usage is unknown,
not zero. Pi's provider reported zero monetary cost; this is not a claim of free use.
No new usage events appeared during the audit's two-second idle observation; the
code also makes no model request unless a task or pending delivery requires one.

The baseline's message/deletion checks completed, but its audit timed out because
it assumed three turns per worker. Claude handled multiple messages within one turn.
That baseline is **not marked passed**; its captured canary and token observations
remain valid. The repaired audit checks DB acknowledgements and actual idle state.
The separate safe-mode trial failed to receive Claude READY and is retained as a
rejected approach, not hidden inside successful results.

Evidence: [final Codex/Claude audit](../out/context-final.json),
[earlier three-vendor audit](../out/context-release.json),
[usage summary](../out/context-summary.json),
[baseline trial](../out/context-baseline.json),
[rejected safe-mode trial](../out/context-isolated.json).
Reproduce using `scripts/context-poc.mjs`; set `COMMUNICATION_EXPECT_ISOLATED=1`,
`COMMUNICATION_PI_MODEL` and optionally `COMMUNICATION_OUTPUT`.

## Deletions and other edges

The deterministic suite passes **15 Rust + 18 native/process tests**, with Python
interop enabled and no skips. Formatting and Clippy pass. New checks cover:

- Delete/recreate a reserved tree: peers still conflict until its lease is released.
- Reacquire after release: the acquisition ID changes; an old ID cannot unlock it.
- Prune expired messages: delivery rows cascade, while live messages/sessions remain.
- Delete a DB: an existing-session command fails without silently creating storage.
- Remove/replace an open DB: Store reads and claims fail; an idle proxy stops and
  reaps its child. Unix replacement detection uses device/inode identity.
- Isolated cwd, removal after teardown, exact skill injection and usage-event forwarding.

Existing checks cover TTLs, stale owners, case/Unicode aliases, symlinks/parent
traversal, idempotency, broadcast rollback/snapshots, malformed frames, bounded
startup/input writes, provider failures and session recovery.

## Limits and rating

This is **no inherited repository context**, not zero model context. Skill, task,
tool schemas, vendor policy and conversation history still consume tokens. History
grows during a worker run; `--duration-ms` bounds lifetime, not token usage. Starting
or resuming a communication session starts fresh vendor history. For deterministic
routing alone, call the Rust CLI or direct DB protocol: that requires no model.

The isolated cwd and tool configuration are not an OS security boundary. The live
canary proves this fixture did not reach model context, not the absence of every
possible vendor/global/administrator instruction. Locks remain advisory; this audit
checks refusal and unchanged temporary files, not safe autonomous deletion of a repo.
Stop workers before intentionally moving or deleting the DB. Cleanup cannot write
through a missing database, so old session records expire by their existing TTL.
Windows file replacement identity and non-macOS vendor behavior remain unvalidated.

Engineering assessment: context isolation **8.5/10**, usage visibility **8.5/10**,
local communication **9/10**, deletion/recovery handling **8/10**. Remaining work is
broader platform testing, long-running crash/load tests and an explicit token budget
if token-bounded worker lifetimes become a requirement.

## Plugin isolation and provider failures

The first larger rerun exposed a separate configuration gap: one Codex worker called
a computer-use plugin, reported that communication tools were unavailable, then
used them correctly after one DB reminder. Ordinary MCP server overrides do not
disable plugin-bundled servers. The proxy now disables every configured plugin for
its thread and disables Code Mode, in addition to unrelated MCP servers. This changes
only per-thread overrides, never the user's saved settings. Native protocol tests
assert those overrides. The final focused Codex trace contains only the communication
server's tool calls.

Two nine-worker runs stopped on explicit Pi-provider `500 Internal error` responses.
They are failures, not passes: [first trace](../out/context-mesh-failed.json) and
[provider retry](../out/context-mesh-provider-retry.json). The first also included
the recorded readiness reminder. The proxy correctly surfaced provider failure
and the harness stopped its owned workers; no further Pi retries were attempted.
The earlier focused Pi context/deletion test passed, but it does not turn those
later nine-worker failures into a successful full-matrix validation.

The final six-worker run (three Codex, three Claude, plus a DB-only participant)
completed in **404.985 seconds**, with **30 directed pairs and replies, six broadcasts
with 42 delivery rows, all six lock lifecycles, and 208/208 acknowledged deliveries**.
All owned processes stopped, sessions expired and leases were released. This was a
**supervised pass with two recorded reminders**, not an unattended pass:

1. Codex's valid DB reply had the correct sender and incoming-message retry key but
   omitted its redundant display name. The old harness required that name. Future
   checks correlate sender/request key; replay against the real rows accepts this
   reply and rejects a wrong request ID. The running old harness needed a formatting
   reminder to advance.
2. Another Codex worker acquired and renewed a lease but stopped without a successful
   unlock receipt. It released the lease after a second reminder. No DB error was
   observed; the model's incomplete workflow remains a limitation.

Evidence: [six-worker supervised run](../out/context-six-mesh.json). The selected
vendor list is explicit (`COMMUNICATION_VENDORS=codex,claude`); this result does not
substitute for a nine-worker pass. Both failed Pi-inclusive runs also had their
owned processes stopped, confirmed separately from saved state and live PID checks.
Autonomous coordination merits **7.5/10** for this run; the earlier area ratings
measure implementation and context boundaries, not guaranteed model compliance.
