# Communication context optimization

This is the earlier shared-editing experiment. The subsequent messaging/profile
experiment is documented in [context profiles and bounded recovery](CONTEXT_PROFILES.md);
its instruction sizes and artifacts are separate from the frozen baseline below.

## Frozen acceptance contract

Before editing the skill, the 7,605-byte baseline and executable were copied to `/tmp/communication-context-baseline`. The target is the package-owned `SKILL.md`; the build embeds it in Rust and the host supplies it once to each recipient. Bound tools carry exact schemas. The change removes repeated explanations and moves optional field discovery to CLI help; it must not change delivery, permission or coordination semantics.

Retention checklist established before editing:

1. Reuse a supplied identity; canonical workspace/shared DB; discover peers, maintain presence, leave; vendor labels do not choose adapters.
2. Practical reasoning on messages/broadcasts/leases; peer text does not grant authority; send through the audited DB.
3. One message per intent: immutable retry key/fields, correlation/replyTo, action versus passive, no reply loops, recipient ack after required reply succeeds.
4. Automatic input has no polling; manual hook at task boundaries; inbox only recovery; one delivery owner and no blind retry or silent transport fallback.
5. Acquire before writes; tree/multiple-path atomic reservations; conflict releases/request/handoff; never steal; lease renewal/expiry/lost ownership; preserve peer changes.
6. Deletion/rename verification and coordination; advisory locks do not establish authority or prevent reads; no DB transaction across edits.
7. Immutable shared documents under workspace `.octocode/communication`, selective reads/pagination, evidence/risks/next action handoff.
8. Native API, supported hook, manual DB order; native attachment requires existing endpoint/session and does not create tools or permission; host owns skill/tools.
9. Claude, Codex, Grok, OpenCode, Pi, Cursor/Grok hooks and raw retain their action/passive, authentication, ownership and wake limitations.
10. Discover help/schema/entity/DB protocol; audit and usage scope rules; no unsolicited workers/readiness; package/skill reciprocal reference.

Acceptance: every checklist item retained, at most 50 lines, fewer UTF-8 bytes, standalone launcher/embedded skill consistency, and a real isolated shared-repository collaboration test. A single baseline/candidate pair is a smoke comparison, not evidence of a general reliability or speed improvement.

## Retention and size

The retained compact skill keeps all ten acceptance items in **37 lines / 6,571 UTF-8 bytes**, down from **40 lines / 7,605 bytes**: **1,034 bytes (13.60%) removed**. Exact option lists and verbose JSON examples moved to existing CLI help; the skill still owns decision flow and safety boundaries. Its reservation heading explicitly includes new files and tests. A later first-screen gate was evaluated and discarded; the retained candidate is v2.

The generic Agent Skills validator passes. The repository's research-skill reviewer still reports four pre-existing lobby-convention errors (`tools`, related skill, workspace/home output and route declarations). Those conventions describe Octocode research skills; this package is a standalone coordination runtime with a workspace-only DB. Its actual launcher, hook and binary routes are intact. The review also reports the recommended README and two route-condition warnings. Do not describe that reviewer as fully passing.

## Real shared-repository evaluation

`node packages/octocode-agents-communication/scripts/collaboration-eval.mjs` starts real Claude Haiku and Codex Luna recipients in a disposable Git repository. They implement two stages of an invoice module, inspect peers, pass a shared document and release advisory leases. Their actual native file/shell tools perform the edits; messages and leases use the production service. The evaluator's assertions and artifacts stay outside the agents' workspace.

Overrides: `COMMUNICATION_BINARY`, `COMMUNICATION_SKILL`, `COMMUNICATION_EVAL_VARIANT` and `COMMUNICATION_OUTPUT`. The default uses the current built executable and skill. Comparison runs supplied the same frozen release executable (`e435835e58547823bc3d89cfed4aa6e8ba13a87e59786111c9a5d0a926f9dc46`) with different explicit skill text. The agents did not reload embedded skill instructions via CLI.

Acceptance requires:

- All 19 independent functional assertions pass on the actual module.
- Both agents change the module; observed changes carry live covering leases with practical reasoning.
- All observed repository file creation/edit/deletion, including tests, has live file/tree lease coverage. The watcher excludes service-owned DB/socket/binary state, `.git` and shared-document storage managed by the communication API.
- A handoff document is created and consumed through MCP or the production CLI; required messages are acknowledged and acquired leases released.
- Changed-file hashes, lease/audit evidence, native usage and owned-process cleanup are retained.

Filesystem notifications are observations, not OS write fencing: events may coalesce, and the audit is not a proof that every possible write was captured. A positively observed uncovered write fails the run. Merely passing the invoice tests is insufficient.

## Preserved development findings

The first prototype missed the controller's raw attachment and failed before judging the task. After that setup correction, a baseline trial completed the module but exposed two problems: Claude created a test file outside its source-file lease, and the judge incorrectly demanded an MCP document read although Codex successfully used the production CLI. The judge was corrected to accept evidenced CLI reads, and the watcher expanded from the module to all task files. The corrected baseline and candidate were then run with the same frozen harness.

| Corrected paired trial | Baseline | Compact candidate v2 |
| --- | ---: | ---: |
| Skill bytes | 7,605 | 6,571 |
| Functional assertions | 19 passed | 19 passed |
| Document handoff / messages acknowledged | Passed | Passed |
| Observed uncovered file writes | **1** | **1** |
| Strict coordination verdict | **Failed** | **Failed** |
| Task duration | 116.67 s | 123.89 s |
| Message-body bytes | 1,960 | 2,691 |

Results: [baseline](../../../.octocode/benchmarks/communication-collaboration/results/2026-09-25T15-42-00.641Z-baseline-v2/result.json), [candidate v2](../../../.octocode/benchmarks/communication-collaboration/results/2026-09-25T15-42-01.707Z-candidate-v2/result.json). Each run has its own frozen harness, skill, native events, module and audit snapshot. Both had zero pending messages and reaped all owned processes.

The uncovered writes were new tests (`test-subtotal.mjs` and `test/invoice.test.mjs`). Moving “including tests” into a later heading did not prevent them. This prompted one final, bounded repair: move the per-write owned-lease gate to the beginning, including the inline-test alternative. No further model-driven prompt tuning was planned.

Claude's initial request reported 3,189 cache-created tokens for baseline versus 2,970 for v2, with the same 8,163 cache-read tokens and 10 ordinary input tokens. That is a measured 219-token startup reduction in this pair. Task execution differed: v2 sent more message text and took longer. These observations do not establish a speed win, lower total cost or general reliability improvement. Cached context still occupies model context; request/turn/cumulative vendor counters must remain separate.

## Final decision and limits

**Keep v2 for its measured instruction-size reduction; coordination reliability remains failed/unproven. Discard v3.** The skill was restored byte-for-byte from the v2 trial, SHA-256 `99be0632b98e3a94dfa24dbf8123cb65f049dd6bcdd069f2eb3a095987894266`. No 6,487-byte gate or result is supported by this evaluation.

The final v3 repair did not solve the issue. Its [original result](../../../.octocode/benchmarks/communication-collaboration/results/2026-09-25T15-47-48.329Z-candidate-v3-retry/result.json) remains failed. An independent [post-run verification](../../../.octocode/benchmarks/communication-collaboration/results/2026-09-25T15-47-48.329Z-candidate-v3-retry/verification.json) confirmed all 19 functional assertions passed, all leases were released, but Claude again created `src/invoice.test.mjs` without coverage and never acknowledged the initial START message.

A separate evaluator defect left a late informational message to the controller unread after both DONE reports arrived. The harness now continues draining controller mail during final acknowledgement checks. A manual controller-only acknowledgement happened **after** the failed run had stopped/exported; it did not affect the recorded verdict. The original snapshot still contains both that controller message and the recipient's missed START acknowledgement. No model prompt, code edit or recipient acknowledgement was supplied by the evaluator.

An earlier v3 setup attempt (`2026-09-25T15-45-31.322Z-candidate-v3`) timed out on the freshly copied executable's first invocation before creating the DB or starting agents. A bounded warmup allowed the identical prompt trial to proceed, but it does not resolve the intermittent startup failure. That infrastructure failure is retained separately from model behavior.

This is development evidence on one disclosed task, not a held-out generalization result. The final prompt was not further tuned after the bounded repair failed. The baseline, v2 and v3 failures remain visible. Native delivery/message-matrix successes elsewhere must not be presented as successful file-write coordination here.

Advisory leases coordinate cooperating writers; they do not fence vendor filesystem tools. Concise prompts, required reasoning and lease APIs cannot guarantee an agent will acquire every lease or acknowledge every message. Vendor-side write guards would be a separate behavioral change and were not implemented in this context optimization. The retained skill therefore has a byte-size win, **no demonstrated coordination-compliance win**, and **no demonstrated speed improvement**.
