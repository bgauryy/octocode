# Octocode TUI contract

The interface separates the persistent conversation, live activity, and session metadata. Pi owns the transcript and streaming. Octocode observes execution as structured events and projects those facts into a compact footer and inspectable history.

## Runtime and rendering

```text
Pi lifecycle / permission / plan / worker observations
  → typed execution events
  → reducer and canonical read models
  → semantic presentation
  → transcript renderers / live footer / status inspector / JSONL
```

| Owner | Responsibility |
|---|---|
| [Execution events](../src/tools/execution-events.ts) | Correlated session, turn, message, tool, skill, file, plan, worker, question, permission, and context events; replay reducer and JSONL serialization |
| [Execution runtime](../src/tools/execution-runtime.ts) | Append-only Pi custom entries and replay of the selected branch; changed plan and worker observations |
| [Lifecycle adapter](../src/tools/lifecycle-ui.ts) | Host execution, outcome, cancellation, usage, and message observations in interactive and headless sessions |
| [Question observer](../src/tools/question-execution.ts) | Each actual prompt and its answer, cancellation, timeout, failure, or durable continuation |
| [Activity presentation](../src/tools/activity-presentation.ts) | One activity priority rule for the footer and motion indicator |
| [Runtime store](../src/tools/runtime-store.ts) | One `zustand/vanilla` store for serializable execution, initialization, MCP, provider-context, background-job, and UI observations |
| [UX snapshot](../src/tools/ux-snapshot.ts) | Immutable projection of runtime, plan, worker, and Awareness facts |
| [Status policy](../src/tui/status-policy.ts) | Attention priority, grouping, density, and viewport budgets |
| [Footer view](../src/tui/footer-view.ts) | Width-bounded semantic rows |
| [Footer controller](../src/extension-ui.ts) | Register once, sample on events and ticks, request repaint |
| [Scroll inspector](../src/tui/scroll-inspector.ts) | Shared keyboard scrolling for session, event, and worker output views |
| [Runtime inspector](../src/tui/runtime-inspector.ts) | Scrollable status and event views, with explicit JSONL export |

Events carry IDs, session/run/turn correlation, sequence, timestamp, visibility, and typed payloads. The reducer rejects stale and duplicate sequence numbers. A cancelled turn closes unfinished tool calls without inventing successful results. A pending durable question survives turn completion; a host answer resolves it separately. Expiry is observed on resume, sampling, and inspection and produces one terminal event.

Pi custom state entries do not enter the model context. Message references preserve user and assistant identity; tool-result references use the call ID. The native Pi session retains complete messages and tool output. Replaying the semantic journal reconstructs execution state; resolving full text requires the corresponding Pi session. The journal does not record private reasoning or duplicate large outputs.

## Three regions

The transcript retains user messages, assistant responses, tool requests/results, skill cards, and significant context or handoff cards. Tool results show a bounded summary by default; **Ctrl+O** opens details. Assistant streaming remains independent of tools.

The live footer names the active operation, such as `Bash yarn test auth` or `Read src/auth.ts:80-240`, and shows factual elapsed time. Permissions and questions have explicit attention states and stop the working spinner. Successful completion restores the underlying plan activity. Settled agent runs clear transient work. Activity text has no repeating dot animation and is not appended to the conversation on each tick.

The session row uses spare footer space for model, elapsed time, tool count, branch, permission mode, and configuration access. Context shows measured used/limit tokens and utilization; unavailable measurements are not presented as zero. Pi's footer provider supplies the branch; passive rendering does not run Git status or diff commands. File-operation events record successful native file mutations. Provider input, output, and cache usage are session totals in the status inspector, distinct from context occupancy.

Durations use seconds, minutes, and hours, such as `6s` or `1m 39s`. An unknown session start displays `—`, never elapsed time since the Unix epoch. GitHub status distinguishes `signed in`, `not signed in`, `checking`, and `status unavailable`; a failed check does not prove that credentials are missing. These diagnostics use available footer space and remain inspectable through `/octocode-status`.

## Plan and worker flows

Use a plan only for complex work with coupled dependencies, coordinated owners,
consequential risk, or substantial work spanning sessions, or when you explicitly
request planning. Routine fixes, a few straightforward steps, and isolated
delegation need no plan or task records.

The plan tool presents its own progress, overview, and review once. **Approve & start** binds the displayed revision and begins implementation. **Request revision** and **Reject plan** require explanatory feedback; **Send comment only** is non-authorizing. Reopen a review through **/configuration → Review plan**. The agent consumes the returned decision; it does not open a second approval prompt. Every checklist row carries its stable task ID so comments and later revisions have a durable anchor.

Linear plans can show completed/total steps. Graph and changing plans show state counts and the current task. Plan/task state comes from the canonical plan read model; presentation never marks a task complete or creates an approval.

Completed task counts mean `done`, not passed verification checks. The UX projection reports completion without a check receipt as `unverified`. Completed plans remain available for inspection.

The footer lists live workers by name and state with their current tool or latest update. Rows keep a stable order while updates arrive. Blocked or failed workers take priority; completed and killed workers remain in the inbox. Explicit `cohortId` values produce bounded, redacted fleet summaries in agent inspection. The viewport bounds the list and exposes overflow through `/octocode-inbox`. The picker offers output, steer, and stop actions according to process liveness. Execution events retain the parent run relationship. The inspector does not create, restart, or stop workers.

Background Bash jobs use the same snapshot and footer pipeline. The ambient row shows
sanitized titles and elapsed time; failures become attention items linked to
`/octocode-status`. The manager retains commands, paths, process handles, and logs and
registers one cleanup with `SessionRuntime`, so no second widget or timer owns the UI.

Compact layouts retain plan progress and the running task beside the current tool.
On short terminals, the combined row leaves room for named workers. Task labels
distinguish running, next, paused, and verifying work; review phases never imply
that implementation has started. Footer updates reuse RPC events and the existing
shared UI clock, without adding model messages or another polling loop.

## Commands

| Command | Result |
|---|---|
| `/configuration` | Browser configuration for MCP, skills, permissions, theme, effort, footer density, and plan review |
| `/octocode-status` | Session, usage, GitHub status, Awareness freshness and shared work, tool outcomes, skills, plan, workers, file operations, and pending decisions |
| `/octocode-status events` | Semantic events for the selected branch, displayed as JSONL |
| `/octocode-status export` | Write `execution-events.jsonl` in the session artifact directory and report its path |
| `/octocode-inbox` | Inspect, steer, or stop a selected worker |
| `/octocode-rewind` | Preview and explicitly apply a local file-history restore |

Inspectors support arrow keys, **Page Up**, **Page Down**, **g** (first), **G** (last), and **/** (filter). **Enter** finishes filter entry; **Escape** clears a filter before closing the view. **Ctrl+C** closes immediately. Filtering applies to complete source lines before wrapping. Empty feature sections are omitted from session status. Inspectors add no model messages. Interactive inspectors require a terminal. Headless hosts retain semantic events in the native session state channel; they do not receive unsolicited terminal output.

## Decision cards

Questions appear inline and require an explicit answer. The card budgets physical rows using terminal height, with a 24-row fallback when height is unavailable. Long questions, option labels, descriptions, trade-offs, and previews remain reachable with **Page Up** and **Page Down**. The visible choices, text editor, and **Enter**/**Escape** controls stay available while context scrolls. Arrow keys change option focus without submitting; paging does not change the selected option. Resizing clamps the viewport to the available height.

Only focused options expand their details. The option window shrinks on short terminals; arrow navigation reaches all choices and the custom-answer row. A recommendation is a starting cursor position, not consent. Cancellation, timeout, and unavailable interaction never authorize a default. Full question and answer text remain in the tool result for later context recovery.

Operation cancellation closes active question cards and selection overlays through
Pi's completion callback. Question cancellation also removes its durable pending
interaction, so a late answer cannot authorize a cancelled operation. Scroll
inspectors require TUI mode; RPC receives an explanatory notification instead of
calling the unsupported custom-component surface.

## Width, attention, and lifecycle

Automatic density uses up to 15% of terminal height, capped at six rows. Compact density uses up to two rows; expanded density uses up to ten. Pending decisions and blocked or failed workers outrank ordinary activity and session metadata. Metadata has no reserved row. Worker state is a separate token before the name, so shortening a long name does not erase `blocked`, `failed`, or `error`. Narrow layouts reserve room for the detail route.

Every selected footer row occupies one physical line. Renderers use cell-width-aware truncation for ANSI, CJK, emoji, and combining characters. [The palette](../src/tui/palette.ts) owns semantic colors; text and state labels remain understandable without color. The working spinner is the only animated decoration.

The footer registers once per session runtime and repaints through `requestRender`. Runtime bindings use Pi's stable session manager; fresh callback context objects share the same clock, tool counts, login state, and footer registration. Render closures perform no I/O or session scans. Octocode adds no persistent widget and no mutable header above the transcript, because competing surfaces add noise and repainting old scrollback can cause a terminal redraw. Session replacement disposes subscriptions; `/tree` restores the selected ancestry before publishing subsequent observations.

Working-indicator initialization follows runtime identity and rendered theme output,
so fresh event contexts do not restart the spinner. Theme changes and replacement
runtimes refresh it. Renderer teardown errors are recorded without skipping the
remaining session resource cleanups.

Awareness means shared work, verification checks, and messages. The footer reader uses the same database, workspace, and actor bindings as Pi's native Awareness tool. During active turns the existing UI clock requests a throttled refresh, at most once every eight seconds. Freshness uses the last successful source read; a cached repaint does not renew that timestamp. Stale or unavailable state links to `/octocode-status` for its age and cached details. Context recovery validates and assembles sources only when a recovery is pending; ordinary turns reuse the frozen policy and deliver changed task context.

Persistence failures produce an explicit status warning while live execution remains usable. Invalid event records fail replay explicitly and mark startup as failed. Export failures report an error. None of these operations authorize a tool, answer a question, or change the plan.

## Verification

[Execution tests](../tests/execution-events.test.ts) cover replay, interleaving, cancellation, identity, and unavailable usage. [Lifecycle tests](../tests/execution-lifecycle.test.ts) cover host observations, resume, pending questions, and persistence failure. [Inspector tests](../tests/runtime-inspector.test.ts) exercise full scrolling and cancellation at 36, 52, 80, 120, and 160 columns. Footer and status-policy tests cover priority, density, stable row identities, and cell-width bounds.

[Controller tests](../tests/execution-footer-controller.test.ts) verify that redraws and resizing use cached external facts and that disposal releases subscriptions. The removed command handlers and standalone dashboard formatters have no compatibility aliases; inspection uses the registered commands above.

Full package validation also requires the installed Pi peer and local workspace dependency builds. Renderer or adapter tests alone do not establish a successful live Pi launch.
