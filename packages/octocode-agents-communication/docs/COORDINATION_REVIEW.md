# Coordination review — 2026-09-25

Historical live results below precede required `reasoning` (schema v3). The current
CLI/DB intent contract and migration checks are documented in [DB.md](DB.md#required-intent).

This review covers the standalone communication package. Awareness was not changed.
The single skill remains 50 lines; schemas and implementation live in the Rust CLI.

## Changes and reasons

| Change | Why |
| --- | --- |
| Atomic `lock_many` | A rename or multi-file task must not reserve half its paths while waiting for the rest. |
| Owner, effective expiry, held IDs and keyed handoff question on conflict | An agent can ask once, release its own reservations, and make progress elsewhere instead of polling. |
| Expired-owner lease cleanup | A long lease must not block work after its owner's presence has expired. |
| Immutable shared documents | Large evidence stays in `.octocode/communication`; messages carry references and recipients read needed byte ranges. |
| SHA-256 and author audit | Recipients detect modified/deleted handoffs, and the DB records who published each document without duplicating its body. |
| Pi tool input over stdin | Large JSON documents must not fail at the operating system's command-argument size limit. |
| Explicit managed-delivery instructions | Automatic recipients handle injected messages; manual inbox inspection is reserved for requested recovery. |

## Evidence

The final live run passed in **523.297 seconds** with two Codex, two Claude and two
Pi workers: **207 messages, 249 acknowledged deliveries, 30 directed worker pairs**,
six broadcasts, six document handoffs, capability questions, owner questions and
all lock lifecycle checks. Every worker delivery had one submitted dispatch and
there were **zero unsolicited inbox reads or supervisor interventions**. All owned
vendor processes stopped; test sessions expired and no leases remained.

The 47,447-byte handoff stayed on disk. Each worker requested a 100-byte window and
received the 47-byte proof tail. Maximum message body was 500 bytes. Direct replies
measured **10.157 seconds median / 24.263 seconds p95**, including model inference
and queues. One Pi turn separately paused for **290.498 seconds** after tool calls,
then recovered without intervention or replay; its exact cause is unproven. It
held no locks during the pause. Fast transport does not guarantee a fast model.

Observed peak context was **19,630–21,481 tokens** for Codex/Pi; Claude did not expose
a comparable context gauge. The report retains cache/input/output counts per worker.
Vendor accounting differs: Codex input includes cached tokens, while Anthropic-style
input/cache fields are separate. Do not add those columns into a misleading shared
cost estimate. Large cumulative cached-input totals reflect repeated provider
conversation use, not repeated dispatch of peer messages.


Automated status: **15/15 Rust tests passed**, formatting and Clippy passed, and
**42 unique CLI/process checks passed across completed full and targeted runs**.
The last completed full suite was 41/42: its remaining generated-vendor startup
fixture was fixed and passed separately. A subsequent unified rerun was stopped
after 30 passes when a copied launcher stalled before shell startup; it is not
reported as 42/42. The copied launcher test now has a 60-second bound. Sampling
showed `_dyld_start` in all 797 samples, before application code. This host-level
validation limitation remains visible despite the passing real-vendor mesh.

- The runtime checks cover transactional contention, aliases, stale acquisition IDs,
  expiry, schema compatibility, message retries, subscriptions and broadcasts.
- CLI/process tests cover hooks, one-time receipts, generic SQLite clients, document
  integrity and traversal, cancellation, process teardown and large Pi tool input.
- [The live exercise](COORDINATION_MESH.md) records real Codex, Claude and Pi results,
  actual tool receipts, per-worker usage and a complete directed message matrix.
- `scripts/lease-crash-poc.mjs` killed a real raw listener with SIGKILL. A peer was
  denied before expiry, then acquired its 120-second lease after the owner's
  60-second presence expired. Recovery took **60,063 ms**. No fixture clock edits.
- `out/coordination-cost.json`: 50 warm CLI sends, including launcher startup,
  validation and SQLite commit, measured **18.77 ms median / 22.03 ms p95**.
  Routing used **zero model calls**. This is not receiver response latency.
- Standard Agent Skills validation passes. Repository skill-review still reports
  four lobby-boilerplate conventions and a README recommendation that conflict
  with this standalone Rust, one-file skill contract; those are not claimed green.

## Blunt review

The first six-worker exercise completed all functional flows, but failed the
strict dispatch-receipt check because Claude also read the manual inbox. No message
was lost; every delivery was acknowledged. Those redundant recovery reads were
still a context-efficiency defect. Both the production proxy role and skill now
state when recovery is appropriate, and the strict rerun also checks zero unsolicited
inbox calls. The detailed exercise report retains both attempts; the final strict rerun passed.

These are advisory reservations, not filesystem fencing. A writer that ignores
expiry can still edit; an owner that deliberately renews a long lease can still
delay peers. Atomic acquisition prevents partial sets but does not resolve cycles
created by independently held older locks unless agents release them as instructed.

File publication and SQLite commit are separate durability domains. A crash can
leave an unregistered document; it is preserved and rejected, never overwritten.
Audit and documents have no automatic retention purge. Same-user malicious SQL or
filesystem changes remain outside the cooperative contract.

macOS stalled some freshly copied executables in `_dyld_start` before application
code. One copied-skill retest took 103 seconds; a fake vendor startup subsequently
hit its real deadline. Fake-vendor tests now preflight their generated interpreter scripts separately,
clear startup artifacts, then retain the same production deadline and assertions.
The live harness can select an
existing built skill and bounds CLI startup; standalone copying has separate tests.
Only macOS ARM64 has been exercised here. Cursor has fixture coverage, Grok has a
prior live hook check, and OpenCode remains research rather than a shipped adapter.

## Ratings

Scores are engineering judgment, not benchmark percentages. They apply to a trusted
local workspace, supported installed vendor versions and cooperating agents.

| Area | Score | Remaining limitation |
| --- | --- | --- |
| Messages and audit | 9/10 | An uncertain external write requires explicit recovery; exactly-once external effects are not promised. |
| Coordination | 9/10 | Intent and handoff cooperation remain model behavior, not authorization or enforcement. |
| Locks and recovery | 8.5/10 | Advisory only; no fairness queue, OS fencing or forced revocation. |
| Context efficiency | 8.5/10 | New IDs and document ranges are compact; vendor conversation history still grows. |
| Architecture and portability | 8/10 | Small Rust/SQLite core; vendor adapters and non-macOS distribution need wider validation. |
| Overall | **8.5/10** | Strong local POC with explicit limits; not a universal host injection or filesystem safety guarantee. |
