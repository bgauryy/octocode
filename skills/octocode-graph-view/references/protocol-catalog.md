Load when choosing a shared task, finding, handoff, decision, or reporting protocol. Source: `skills/common`; installed copies are generated.

## Catalog

Cite the protocol name with task-specific slots in the receiver's words. Fit: 5 default, 4 strong in its niche, 3 situational or covered by a higher row, 2 rarely better than a higher row. These are editorial ratings; behavioral claims need `octocode-eval-benchmark`.

### Describe and decide

| Need | Protocol line | Fit | Not for |
|---|---|---|---|
| Issue/task: bug, finding, backlog item, tool defect | `5W2H: what, where (path:line), when (trigger), who or what it affects, why (cause), how (reproduce or fix), how much (impact)`; "unknown" for unverified slots | 5 | A lookup one read answers |
| Incomplete information | `Facts, assumptions, unknowns`: what evidence shows, what you assume, what nobody checked; never present a guess as a fact | 5 | A fully verified answer |
| Research finding, conclusion | `Claim–evidence`: each claim beside its `path:line`, command result, or source link | 5 | Opinions marked as such |
| Cause of one failure | `Hypothesis test: claim, strongest rival, distinguishing check, result (confirmed, dismissed, unresolved)` | 5 | A lookup one read answers |
| Recurring problem | `5 Whys`: ask why until a cause you can change; verify each answer | 3 | One-off failure: a single chain can stop at a convenient cause; prefer hypothesis testing |
| Expected behavior, acceptance | `Given–When–Then: given <state>, when <action>, then <observable result>` | 4 | One command proves it ("exits 0") |
| Architecture choice | `ADR: context, decision, status (proposed, accepted, superseded), consequences` | 4 | Every document |
| Choice someone else must make | `Decision request: options, tradeoffs, recommendation, default if no answer` | 4 | Choices within your own authority |
| Decision with several stakeholders | `DACI: driver, approver, contributors, informed` | 3 | A parent and its workers: the parent or user approves |
| Feedback on a plan | `Objection (blocks; states the consequence) vs concern (non-blocking)` | 4 | Style notes |
| Review comment | `Conventional Comments: label (issue, suggestion, question, nitpick, praise), blocking or not (marked in the comment), location, why` | 4 | Findings that need evidence, impact, confidence, repair (use 5W2H) |
| Independent judgments | `Delphi: same evidence, independent first positions, then share; keep dissent` | 3 | One reviewer |
| Risk before committing | `Premortem: assume it failed; list reasons independently` | 3 | Scopes a domain checklist already covers |

### Delegate and coordinate

| Need | Protocol line | Fit | Not for |
|---|---|---|---|
| Delegation brief | `Task contract: objective, inputs, constraints (scope, authorization), output`, plus a Definition of Done; the worker picks the method | 5 | Work you do yourself |
| Completion of a bounded task | `Definition of Done: <observable checks>` | 5 | Open-ended search (use a stop condition) |
| Open-ended search or iteration | `Stop condition: done when <checks>; escalate when <boundary: budget spent, no new lead, missing decision>` | 5 | A bounded task with a Definition of Done |
| Goal quality | `SMART`: specific, measurable, achievable, relevant, time-bound; check the goal or Definition of Done, no separate field | 3 | Goals where "achievable, relevant" change no action |
| Costly delegation | `Budget contract`: time, token, or tool-call limits; at exhaustion return partial results and what remains | 4 | Cheap tasks |
| Large background | `Context manifest`: paths or URLs, version or commit, why each matters | 4 | Material that fits in the brief |
| Shared responsibilities | `RACI: responsible (does), accountable (owns), consulted, informed` | 3 | One worker; a Pi parent is accountable by default |
| Consequential or ambiguous instruction | `Check-back`: the receiver restates objective and Definition of Done before it edits or spends | 4 | Routine, reversible steps |

### Report and hand off

| Need | Protocol line | Fit | Not for |
|---|---|---|---|
| Any result, reply, report | `BLUF`: conclusion and confidence first, then evidence, limits, next check | 5 | — |
| Transfer of unfinished work | `Handoff packet: current state, evidence, blockers, next action, constraints, next-call ids`; no raw logs, private reasoning, or abandoned branches | 5 | Finished work (report BLUF) |
| Problem that needs someone else's action | `SBAR: situation, background, assessment, recommendation` | 4 | A problem you can resolve yourself |
| Progress on shared state | `Delta update`: only what changed since the last update (phase, change, blocker, next action) | 4 | The first report |
| Deviation from plan | `Exception report`: report only deviations, each in SBAR | 3 | Normal progress; the name adds only the "report exceptions" trigger to SBAR |
| Work that spans sessions or interruptions | `Checkpoint–resume`: goal, done, in progress, next, open questions, files | 4 | Short tasks |
| Failure or retrospective | `After Action Review: intended, happened, why different, next` | 3 | Every successful run |
| Completed-work summary | `STAR: situation, task, action, result` | 2 | Agent consumers: BLUF puts the result first; retrospectives use After Action Review |

### Loops and failures

| Need | Protocol line | Fit | Not for |
|---|---|---|---|
| Transient failure | `Bounded retry`: retry limit, backoff, then escalate; a reasoning retry needs a revised hypothesis | 4 | Permanent errors: fix or report them |
| Improving a repeatable workflow | `PDCA`: plan the change, do it, check against a baseline, act (keep or revert); name the command in each step | 3 | One-off tasks |
| Plan invalidated mid-execution | `OODA`: observe fresh state, orient, decide, act | 2 | Tool loops that already re-read state |
| Repeatable request with side effects | `Idempotency key`: one key per effect; the receiver drops duplicates | 2 | Agent wording: it belongs in tool and protocol design (the service contract owns deduplication) |

### Writing

| Need | Protocol line | Fit | Not for |
|---|---|---|---|
| Page kind | `Diátaxis: tutorial, how-to, reference, explanation; one per page` | 4 | Agent instructions |
| Rule strength | Plain imperative; uppercase MUST/NEVER only for safety, permissions, secrets, hard contracts | 5 | Emphasis |

Avoid brevity codes and invented abbreviations, loop names (OODA, PDCA) in place of steps that name commands, and full agent-language ontologies when a few verbs suffice.

