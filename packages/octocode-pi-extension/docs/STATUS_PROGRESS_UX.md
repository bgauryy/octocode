# Adaptive status and progress UX

> Status: core implementation shipped. `UxSnapshotV1`, event reconciliation, adaptive priority/budget policy, bounded worker aggregation, one-line cell-safe rendering, production footer wiring, durable plan/worker transcript cards, and a real Pi 0.84.4 launch pass are implemented and covered by focused fixtures. Durable peer/local reconciliation and the complete state/layout matrix remain acceptance debt. [UI.md](UI.md) is canonical for shipped behavior.

This design gives users one truthful answer to four questions:

1. What is Octocode doing now?
2. How far has the work progressed?
3. What needs my attention?
4. Where can I inspect or act on the details?

It covers status and progress for the Pi footer, activity indicator, transcript, plans, tasks, subagents, peer messages, verification, and noninteractive output. It does not replace canonical plan, worker, or Awareness storage.

## Decision

Derive one versioned **UX snapshot** from existing authoritative state, then project that snapshot through a **salience- and space-aware policy** to every surface.

The default footer keeps current work and attention visible within a row budget. It lists active worker names, states, and updates in stable order. Blocked, failed, or input-waiting work can preempt lower-priority metrics. Full detail remains available through the plan page, inbox, transcript, and commands.

This design replaces two brittle extremes:

- an unbounded footer that renders every worker and category; and
- a fixed compact footer that hides important state whenever the layout changes.

## Current repository status

The existing architecture has strong foundations but an inflexible final projection.

| Area | Current behavior | Design consequence |
|---|---|---|
| Canonical plan state | `tools/plan-read-model.ts` exposes phase, revision, task states, dependencies, receipts, pending interactions, and summary counts to terminal, browser, RPC, prompt, and Markdown projections. | Keep this owner. Adapt it into the UX snapshot; do not create another plan store. |
| Footer controller | `extension-ui.ts` is the production owner that reads runtime, plan, worker, Awareness, identity, and context state before calling `tui/footer-view.ts`. | Keep one controller and one registered footer component. |
| Footer layout | `tui/status-policy.ts` applies automatic, compact, or expanded row budgets; active workers have named rows and blocked/failed workers take priority. `tui/footer-view.ts` renders one physical line per selected row. | Keep selection pure and rendering bounded; route overflow to the complete inbox. |
| Foreground activity | `tools/runtime-store.ts` remains the canonical discriminated activity store. `tools/execution-events.ts` provides typed lifecycle events, sequence rejection, and replay; `tools/execution-runtime.ts` binds the selected Pi branch. | Wire producer-owned source sequences and leases through every activity publisher before claiming end-to-end stale-completion protection. |
| Progress | `tools/ux-snapshot.ts` classifies linear, graph, dynamic, and indeterminate progress. The policy shows a denominator only for a stable linear plan and uses state counts for graph or dynamic work. | Extend canonical verification/task fields when all plan surfaces can consume the richer states. |
| Agents | `tools/ux-snapshot.ts` preserves process/result precedence, assignments, active operations, messages, elapsed time, and update time. The footer names live workers and their updates; blocked/failed outcomes remain visible with `/octocode-inbox`, and done/killed outcomes remain briefly visible before expiring. | Keep the inbox as the complete ledger and gate actions on process liveness. |
| Messages | Worker queued counts and cached Awareness unread counts merge only in the UX snapshot and policy; canonical stores remain separate. Plan changes, worker messages, and worker state transitions append durable, zero-context-cost transcript cards with consecutive reply deduplication. | Extend the same transition grammar and durable coalescing contract to peer notifications. |
| Detail | `/octocode-inbox`, plan HTML, terminal plan output, Awareness detail, `/configuration`, and transcript routes remain complete drill-down surfaces. The one-line renderer promotes routes before optional tail detail. | Preserve a real detail route before reducing ambient detail further. |

### Structural verification snapshot

The current audit used lexical search, AST structural search, LSP references, exact reads, and file-graph paths:

- AST search found 24 `setManagedActivity(...)` call sites and 11 `setManagedStatus(...)` call sites.
- Current implementation: `extension-ui.ts` owns one production `renderFooterView(...)` call. Plan rows are selected by `status-policy.ts`; the separate legacy plan footer builder was removed. See [UI.md](UI.md) for the current contract.
- LSP resolved `setManagedActivity` to 32 references across lifecycle, plan, rehydration, and tests.
- LSP resolved `getCurrentPlanReadModel` to 31 references across 10 files, confirming that it is already the shared plan presentation boundary.
- LSP resolved the footer renderer to `extension-ui.ts` as its production consumer and resolved agent inbox registration to `index.ts`.
- Graph path checks found direct imports from `extension-ui.ts` to the plan read model, footer view, and agent tools; from `index.ts` to the agent inbox; and from the plan tool to the runtime renderer.
- Graph results for these paths were complete. Whole-workspace graph coverage remained partial because of one nonliteral CommonJS import and unresolved test-utility imports, so this audit makes no universal dead-code or reachability claim.

## Product principles

### 1. Show your work, not the machinery

Lead with goal, active task, meaningful progress, and next action. Tool names, model identifiers, prompt overhead, branch details, and clocks are secondary unless they explain a delay or risk.

### 2. One fact, one owner

Canonical stores own facts. The UX layer only derives and ranks projections.

- Plan and task truth: `tools/plan-read-model.ts` and Awareness task state.
- Worker truth: the worker ledger and normalized worker result.
- Session activity: `tools/runtime-store.ts`.
- Peer and verification truth: Awareness read models.
- Rendering and density: the TUI projection policy.

No renderer mutates workflow state. No notification becomes canonical evidence.

### 3. A bounded ambient surface, complete drill-down

The footer has a viewport budget. It is not a dashboard squeezed under the editor. The footer can aggregate state when the summary is explicit and reversible:

```text
blocked · atlas · /octocode-inbox
running · nova · tool localSearch · /octocode-inbox
queued · rhea · follow-up ready · /octocode-inbox
```

This is more truthful than either eight persistent rows or an unexplained `+7`.

### 4. Attention preempts decoration

Input requests, permission decisions, stale authority, failed checks, blocked active work, and failed workers outrank identity and metrics. Normal completions do not interrupt you unless they complete background work you are waiting for.

Session metadata uses remaining capacity; compact mode does not reserve a row for it. Worker state precedes the name as a separate token. Repainting cached Awareness counts preserves their original observation time.

### 5. Progress must state its confidence

A stable linear plan can show `3/8`. A conditional or parallel graph shows state counts such as `3 done · 2 active · 1 ready · 2 blocked`. Indeterminate work shows activity and elapsed time instead of inventing percentages or ETAs.

### 6. Stable while ticking, adaptive when meaning changes

Elapsed time and spinner frames can repaint text but must not change footer height. Repack rows only on a semantic transition, resize, density change, or attention change.

### 7. Text carries meaning

Color and glyphs reinforce state but never own it. Every state remains understandable with `NO_COLOR`, a narrow terminal, logs, and assistive technology.

## Unified UX metadata

Introduce a derived `UxSnapshotV1`. The name is illustrative; the important contract is one immutable snapshot per render epoch.

### Snapshot sections

| Section | Required metadata | Purpose |
|---|---|---|
| Session | phase, current activity, elapsed time, context pressure, observation time | Answers “is it alive?” |
| Goal | concise goal, current milestone, next action | Keeps your objective above implementation detail. |
| Plan | stable ID, phase, revision state, total/done/active/ready/blocked/verifying/failed counts, dynamic flag | Describes orchestration without flattening task states. |
| Tasks | stable ID, index, label, status, dependencies, owner, verification state, updated time | Supports footer selection and detail views. |
| Agents | stable ID, label, normalized state, assignment, active operation, pending messages, elapsed time, updated time | Supports honest parallel-work summaries. |
| Attention | kind, severity, actor, concise reason, required action, detail route, created time | Drives preemption and notifications. |
| Messages | unread count; queued count; latest relevant sender and subject; detail route | Unifies worker and peer message visibility. |
| Provenance | source owner; source revision or sequence; observed time; stale-after policy | Prevents stale projections from looking current. |

### Event envelope

Producers publish semantic transitions into an ephemeral projection stream. They do not publish preformatted footer strings.

Each event has:

- stable event, entity, and correlation IDs;
- entity kind: session, plan, task, agent, check, interaction, message, or tool;
- previous and next semantic states;
- source owner and monotonically increasing source sequence;
- observed time and optional expiry;
- user-facing label and optional evidence summary;
- severity and whether you must act;
- detail route, such as plan, inbox, checks, configuration, or transcript.

The snapshot reducer rejects stale sequences and coalesces repeated heartbeats. Durable stores remain authoritative after reload; the event stream is only a responsive projection mechanism.

## Progress semantics

### Plans

| Plan shape | Footer representation | Do not show |
|---|---|---|
| Stable linear sequence | `Plan 3/8 · task 4 running: Verify restore` | ETA unless measured from comparable completed work. |
| Parallel dependency graph | `Plan · 3 done · 2 active · 1 ready · 2 waiting` | A single percent that implies serial order. |
| Conditional or changing plan | `Plan · 3 done · scope changing` | A fixed denominator or step indicator. |
| Awaiting review/input | `Needs you · Review plan rev 0284e84f` | Motion that suggests implementation is running. |
| Verifying | `Plan verifying · task 4 verifying: Check restore` | “Complete” before observed checks succeed. |
| Failed or blocked | `Blocked · task 5 · ref publication conflict` | Generic `failed` without cause and next action. |

A task becomes complete only after it satisfies its owning completion contract. A worker saying “done” changes worker state; it does not complete the parent task.

### Long-running or indeterminate work

Acknowledge within 100 ms when possible, then show:

- a verb and object: `Indexing repository`, not `Processing`;
- elapsed time;
- a heartbeat or changing completed-unit count;
- the last meaningful milestone when available;
- a cancel or detail route when the operation supports it.

If no heartbeat or milestone arrives before a policy threshold, change the label to `No update for 30s` rather than continuing an unqualified spinner.

### Verification

Verification has distinct `pending`, `running`, `passed`, `failed`, and `waived` states. Waived checks require a visible reason. Completion messages include observed check counts, not only a success glyph.

## Surface allocation

| Surface | Owns | Persistence | Noise rule |
|---|---|---|---|
| Motion indicator | Immediate acknowledgement that work is active | Ephemeral | No task detail and no duplicated footer label. |
| Footer | Current goal, active work, progress summary, and attention | Ephemeral snapshot | Bounded by viewport; no tick messages. |
| Inline decision card | One blocking human decision and its consequences | Transcript-linked | One focus owner; no competing motion. |
| Transcript milestone | Started, materially changed, blocked, failed, or completed transition | Durable | Coalesce repeated updates; never append spinner ticks. |
| Notification | Background completion or action required while focus is elsewhere | Ephemeral OS/TUI signal | Warning/error/action only; avoid routine success spam. |
| Inbox | Worker and peer conversations, queued work, actions, retained result | Session or durable ledger | Full detail on demand. |
| Plan page/command | Complete dependency graph, contracts, checks, decisions, and history | Durable artifact/read model | No ambient size constraint. |
| Noninteractive output | Ordered plain-text transitions and final receipt | Durable log/stdout | No animation; stable machine-readable fields when requested. |

## Adaptive footer policy

### Budget

The footer consumes a soft viewport budget rather than a fixed row count:

- target no more than 15% of visible terminal height;
- hard maximum of six rows in automatic density;
- at least one row when any activity or attention exists;
- user-selectable compact and expanded modes override the target, not truth priority;
- footer height stays stable during heartbeat-only repaints.

### Priority

1. **P0 — user action or safety:** input, approval, permission, stale authorization, destructive-operation gate.
2. **P1 — active outcome:** current goal/task, verification, blocked active work, failure.
3. **P2 — collaboration:** named live workers and unread messages; blocked/failed workers rank at P1.
4. **P3 — operational context:** context pressure, degraded runtime, credentials, repository state.
5. **P4 — diagnostics:** model, session clock, prompt overhead, server/tool counts.

Higher-priority items preempt lower-priority items. Within a priority, prefer newer state tied to the current goal. The policy is deterministic and pure so permutation tests can prove it.

### Row packing

- Preserve state and required action before labels and elapsed time.
- List live workers by name, state, and current update; use stable worker IDs to order normal rows.
- Keep full finished-worker details in the inbox. Show a short-lived done/killed outcome row, then expire it; count omitted live rows and preserve their inbox route.
- Combine plan progress, running task, and current tool in compact layouts so worker names have room.
- Merge duplicate local-plan and shared-Awareness counts by stable task identity.
- Never truncate `blocked`, `failed`, `input needed`, or the action route.
- Truncate long labels with an ellipsis; expose the full label in the plan or inbox.
- Wrap explanatory helper text in decision/detail surfaces, not in the ambient footer.
- On resize, recompute from the same snapshot; do not change underlying state.

### Example projections

Wide, active plan:

```text
blocked · atlas · dependency unavailable · /octocode-inbox
Plan 2/10 · task 4 running: Add bounded restore I/O · plan
running · nova · tool localSearch · /octocode-inbox
queued · rhea · follow-up ready · /octocode-inbox
ctx 61% · main (12 changed) · perm default
```

Narrow:

```text
blocked · atlas · inbox
Plan 2/10 · task 4 running… · plan
```

Waiting for you:

```text
Needs you · Start plan rev 0284e84f
Plan review ready · exact revision pending
```

Parallel work without attention:

```text
running · atlas · Reading callers · /octocode-inbox
running · nova · Checking tests · /octocode-inbox
queued · rhea · follow-up ready · /octocode-inbox
```

Verification failure:

```text
Verify failed · 5/6 passed · history-ref-atomicity
Next · inspect check output
```

## Message grammar

Every user-facing transition follows:

```text
<actor> <action> <object> · <observed result> · <next action when needed>
```

Examples:

- `Octocode started task 4 “Bound history I/O” · 3 dependencies satisfied.`
- `atlas finished “Map footer consumers” · awaiting parent verification.`
- `Parent verified atlas output · 5 source paths confirmed.`
- `Plan blocked at task 5 · ref publication conflict · inspect plan.`
- `2 peer messages unread · latest from reviewer · open inbox.`
- `Restore checks passed · 18/18 · task 6 complete.`

### Message rules

- Name the actor when multiple agents or systems are active.
- Use verbs that describe observable work: reading, indexing, testing, waiting, verifying.
- Distinguish `queued`, `delivered`, `read`, `acknowledged`, and `resolved` messages.
- Distinguish worker completion from parent verification.
- Include stable short IDs only when needed to disambiguate.
- Include a next action only when you or the parent agent can act.
- Coalesce repeated transitions by entity and state; keep the newest evidence.
- Preserve errors until acknowledged or superseded; do not let a later heartbeat erase them.

## Reconciliation and freshness

The projection layer must resolve competing updates explicitly:

1. Reject an event whose source sequence is older than the snapshot’s sequence for that entity.
2. Prefer canonical plan/Awareness state over ephemeral activity after reload.
3. Treat process state and normalized worker result as separate inputs; derive one display state with a documented precedence table.
4. Expire transient tool and heartbeat state by lease; never expire durable blockers or unread messages silently.
5. Mark stale external state as stale with its last observation time.
6. Rebuild the snapshot from canonical stores on session start, resume, fork, and compaction recovery.
7. Emit one reconciliation milestone only when the visible result changes.

## Proposed implementation boundaries

These names are targets, not an instruction to duplicate current owners.

| Boundary | Responsibility |
|---|---|
| `tools/ux-snapshot.ts` | Pure adapters from runtime, plan, worker, and Awareness read models into `UxSnapshotV1`. |
| `tools/execution-events.ts` | Typed execution events, sequence rejection, and replay of the semantic journal. |
| `tui/status-policy.ts` | Pure priority, grouping, row-budget, and width policy. |
| `tui/footer-view.ts` | Render selected semantic rows only; no state reads or ranking. |
| `extension-ui.ts` | Read one snapshot and register/repaint the one footer component. |
| `tools/agent-inbox.ts` | Complete worker/message detail and actions. |
| `tools/plan-read-model.ts` | Remains canonical for plan/task presentation. Add fields here only when every plan surface needs them. |

Do not add another persistent database, a second footer, or renderer-specific status stores.

## Validation matrix

### State combinations

Cover at least:

- no plan, draft, review, accepted, executing, verifying, complete, and failed plans;
- no task, ready, running, dependency-blocked, verification-blocked, failed, cancelled, and done tasks;
- 0, 1, 2, 8, and 100 agents with mixed process/result/message states;
- no messages, queued worker messages, unread peer messages, acknowledged messages, and resolved threads;
- normal, degraded, failed, and stale runtime sources;
- simultaneous input request, context pressure, worker failure, and verification debt.

### Layout combinations

- widths: 20, 28, 36, 52, 80, 120, and 160 cells;
- heights: 10, 20, 40, and 80 rows;
- automatic, compact, and expanded density;
- resize while active, resize while blocked, and resize during a worker burst;
- color, `NO_COLOR`, non-TTY, and log output;
- long ASCII, CJK, emoji, combining characters, and ANSI-safe labels.

### Behavioral acceptance

- Every P0 item remains visible or names an explicit detail route.
- Automatic footer height never exceeds its viewport budget.
- Heartbeat-only updates never change footer height.
- Normal agent count does not produce unbounded rows.
- Blocked and failed agents remain individually identifiable.
- One semantic fact is not repeated across footer rows.
- Out-of-order events cannot regress visible state.
- Dynamic plans never show a misleading percentage or fixed denominator.
- A worker completion never marks its parent task complete.
- Transcript output contains no spinner frames or duplicate tick messages.
- Noninteractive output contains no animation or invisible prompt.
- Every rendered line is cell-width safe.
- Snapshot, footer, transcript, plan page, and RPC fixtures agree on entity IDs and semantic states.

### Measured UX checks

Track these as test or benchmark outputs rather than visual judgment:

- time to first acknowledgement for long work;
- footer rows and cells consumed per viewport;
- number of durable messages per semantic transition;
- percentage of attention states visible without opening detail;
- stale-event rejection count;
- duplicate-fact count across ambient rows;
- scroll-position changes during heartbeat-only updates;
- user actions needed to reach full plan, worker, check, or message detail.

## Migration order

1. Freeze current footer/message permutations and add failing adaptive-budget fixtures.
2. Build `UxSnapshotV1` as a pure projection beside existing models.
3. Add sequence/freshness reconciliation and prove reload, fork, and out-of-order behavior.
4. Replace fixed footer row assembly with the pure status policy.
5. Bound named worker rows and preserve the full inbox route for overflow.
6. Normalize transcript and notification wording through the message grammar.
7. Merge local worker and durable peer attention counts without merging their canonical stores.
8. Run the full layout/state matrix and a real interactive Pi session.
9. Update [UI.md](UI.md) to make the shipped adaptive behavior canonical and delete superseded rigid rules.

Each step must preserve a real detail route before reducing ambient detail.

## Alternatives considered

### Keep every worker and category visible

Rejected. It is complete only by occupying unbounded terminal height. It scales with implementation entities rather than user attention and conflicts with status-bar guidance to limit items.

### Use a fixed one- or two-line footer

Rejected. It is predictable but hides blockers, input requests, or parallel-work context when several high-priority states coexist.

### Move all status to a dashboard

Rejected. A dashboard is appropriate for detail, but users still need immediate acknowledgement, current activity, and attention while reading the transcript.

### Adaptive salience projection

Chosen. It preserves urgent truth, bounds ambient space, supports parallel work, and remains testable because ranking and packing are pure deterministic policies.

## Research basis

- [VS Code status bar UX](https://code.visualstudio.com/api/ux-guidelines/status-bar): use short labels, limit items, separate primary workspace state from secondary context, and do not turn the status bar into a notification stream.
- [Command Line Interface Guidelines](https://clig.dev/#progress): acknowledge quickly, show progress for long work, keep parallel progress comprehensible, avoid animation outside a TTY, and use symbols sparingly.
- [Carbon progress indicator](https://carbondesignsystem.com/components/progress-indicator/usage/): show current and total steps for stable linear flows; do not use that model for conditional or reorderable work; use meaningful action labels and explicit error/helper text.
- [NN/g: Visibility of system status](https://www.nngroup.com/articles/visibility-system-status/): communicate state continuously enough for users to understand whether an action worked and what to do next, while keeping irrelevant backstage detail out of the primary surface.

The W3C status-message pages blocked automated retrieval during this research pass. Accessibility requirements above therefore rely on the repository’s existing text-without-color contract and Carbon’s tested keyboard/screen-reader guidance; a later implementation RFC must add a separately verified screen-reader announcement contract.
