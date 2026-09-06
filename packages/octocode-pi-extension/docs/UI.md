# Octocode TUI design

This page is the canonical design contract and widget inventory for the Octocode Pi terminal interface. `src/tui/` is the canonical rendering layer; tool modules supply state and event handlers but do not own layout primitives.

The TUI is conversation-first: transcript content is durable, decisions appear inline at the point of interruption, persistent state has one owner, and detailed navigation uses temporary overlays or explicit commands.

## Design goals

- Keep your task and the next required action visible before metrics or decoration.
- Give each fact one stable owner. Do not repeat lifecycle, plan, task, or agent state across surfaces.
- Make every action keyboard-complete, cancellable, and understandable without color.
- Adapt to narrow editor panes and wide terminals without edge-to-edge reading measures.
- Render from state. Rendering must not start I/O, mutate workflow state, or scan the session.
- Keep noninteractive and RPC use deterministic. A missing TTY must never leave an invisible prompt waiting for input.
- Show progress quickly, but reserve motion for active work and stop it when the work stops.

## Component and state architecture

- `src/tui/components.ts` owns the pure functional component contract, responsive inline rows, stacks, and cell-perfect closed frames. Consumers import directly from the defining TUI modules.
- `src/tui/footer-view.ts` owns the unified persistent footer. `extension-ui.ts` projects activity, exact context use/current maximum, compact plan/task progress, every visible worker, attention, identity, and settings into it; diagnostic telemetry stays in commands.
- Call, result, and message renderers use `makeComponentRenderer` and the same functional component contract, so width enforcement and invalidation behavior are uniform.
- `runtime-store.ts` is the Zustand source of truth for initialization, statuses, notices, foreground activity, context composition, MCP progress, and footer metrics. Renderers subscribe or read snapshots; they do not keep parallel UI state. Pi working visibility is derived from foreground activity and carries no second message value.
- Plan and agent mutations request a footer repaint through the shared runtime state.
- Box drawing is centralized. New widgets must use `renderFrame`; legacy left-rail cards use `closeFrameLines` during migration. Both reserve the right edge before truncation and preserve the IME cursor marker.

## Surface hierarchy

The visual order is also the attention order:

1. Transcript: user messages, agent responses, tool rows, and durable completion cards.
2. Inline decision: one focused `askUser` card at the bottom of the conversation.
3. Editor: the normal input surface when no decision owns focus.
4. Footer: priority-ordered state rows plus one row for every visible worker.
5. Overlay: temporary navigation or management opened through an explicit action.
6. Browser companion: optional rich review, opened only after an explicit choice.

Only one interactive surface owns keyboard focus. Closing or submitting that surface returns focus to the editor. An overlay must not obscure an unresolved inline decision.

## Widget inventory

| Family | Widget and owner | Contract |
|---|---|---|
| Session chrome | Terminal title and session banner (`branding/`, `index.ts`) | Identify the session once. Never animate or repaint above the transcript. |
| Activity | Foreground projection (`runtime-store.ts`, `runtime-renderer.ts`) | Show Thinking, Researching, Input needed, Planning, Reviewing, Ready to start, Working, Verifying, Blocked, Complete, or Failed from one Zustand slice; motion only for active work. |
| Transcript | Activity and tool rows (`tui/cli-design.ts`, render helpers) | Call → running/update → outcome. Show user-facing lifecycle state, never private model reasoning text. |
| Transcript | Inline images (browser and Chrome tool renderers) | Render only when expanded; provide a text placeholder when the terminal cannot display images. |
| Transcript | Conversation cards (compaction and handoff renderers) | Keep the summary durable and the payload collapsible. |
| Decision | Single-select `askUser` (`ask-user-tool.ts`) | Recommended choice receives initial focus; only that row expands optional nuance and trade-offs. The widget supports arrow keys, number keys, Enter, filtering, disabled reasons, and free text. |
| Decision | Multi-select `askUser` (`ask-user-tool.ts`) | Space toggles, Enter confirms, min/max validation stays inline, and selected count remains visible. |
| Decision | Text and form `askUser` (`ask-user-tool.ts`) | Use Pi's input component for cursor movement, paste, graphemes, validation, and IME positioning. |
| Decision | Plan/RFC review (`plan-tool.ts`) | Ask only for clarification or proposal review. One Start decision binds the exact revision and starts implementation. Set/start/complete do not ask presentation-only questions. |
| Status | Compact plan (`plan-tool.ts`, `extension-ui.ts`) | Show progress and current work in the footer. Explicit compact inspection prioritizes running tasks, then runnable tasks, then blocked backlog. Markdown, browser, and RPC retain the complete plan. |
| Navigation | Shared select overlay (`ui-overlays.ts`) | Search visible labels and descriptions; preserve focus through filtering; cancel with Escape or Ctrl-C. |
| Navigation | Shared multi-select overlay (`ui-overlays.ts`, `multi-select-list.ts`) | Use the same focus, selection, validation, and cancellation language as `askUser`. |
| Navigation | Command palette (`command-palette.ts`) | Filter all public commands and direct actions; dispatch the selected command through the normal message path. |
| Workers | Worker inbox and agent inspection (`agent-inbox.ts`, `agent-tools.ts`) | Pick → inspect → steer or stop. Footer rows show every visible non-killed worker, attention first; commands retain the complete ledger and evidence. |
| Configuration | Effort dial (`effort-dial.ts`) | Show the current value, explain each choice, and persist the selected level. |
| Safety | MCP consent and removal pickers (`mcp-tool.ts`) | Name the external process or server and make cancel the safe exit. Never mutate when interactive consent is unavailable. |
| Recovery | Checkpoint picker (`rewind-command.ts`) | Identify snapshots by time and intent; distinguish file restoration from conversation rewind. |
| Editor | Mention and plan-step autocomplete (`autocomplete-providers.ts`) | `@` selects workers or skills, `#` selects plan steps, and all other input delegates to file completion. |
| Editor | Watch mode (`ai-watch.ts`) | Convert explicit `AI!` comments into steer or follow-up messages without stealing editor focus; injected prompts are bounded and point back to the source when markers are omitted. |
| Persistent state | Unified footer (`tui/footer-view.ts`, footer registration in `extension-ui.ts`) | Responsive state rows plus one row per visible worker. Promote activity, warnings, exact context, current work, identity, and settings; truncate individual segments at the available width rather than hiding them behind `+N`. |
| Discovery | Dashboard and command guide (`index.ts`, `commands-command.ts`) | Present health first, then the smallest useful next actions. The live command registry owns command inventory. |
| Feedback | Inline validation and notifications (`ask-user-tool.ts`, `desktop-notify.ts`) | Keep recoverable validation next to the control; reserve desktop notifications for completion, failure, or blocked work. |
| Export | Branded HTML export (`export-command.ts`) | Produce a sibling artifact without changing transcript state. |

## Responsive layout

Decision cards align with the transcript and use a bounded reading measure:

| Terminal width | Inline decision card |
|---:|---|
| Under 80 columns | Use the full available width and compact key help. |
| 80 columns or wider | Use an 80-column, left-aligned reading measure. |

Shared picker overlays use a 72-column target, 32-column minimum, two-cell outer margin, and at most 70% of terminal height. Select lists show at most eight rows by default. Descriptions, previews, and trade-offs appear only for the focused item and count against the visible-row budget.

The footer degrades by priority: keep the required action and error state, then exact context and current work, then passive metrics. Worker attention rows sort first. Never truncate Enter, Escape, or cancellation guidance before optional shortcuts.

## Input and focus

| Input | Meaning |
|---|---|
| Up/Down or Ctrl-P/Ctrl-N | Move one selectable row in logical reading order. |
| Enter | Select or submit the focused control. |
| Escape or Ctrl-C | Escape clears an active filter first; from a choice-backed custom answer it returns to the choices. Otherwise cancel the active decision or overlay. |
| `/` | Start explicit filtering in `askUser`; shared overlays filter as you type. |
| `1`–`9` | Select or toggle the corresponding visible option when the UI shows numbers. |
| Space | Toggle the focused multi-select option. |
| `a` / `i` | Select all or invert multi-select choices when constraints allow it. |

The focused row always has a cursor glyph and a contrasting rail or prefix. Color reinforces focus but never carries it alone. Disabled rows remain visible and include a reason. Key help describes only actions available in the current state.

## State and feedback

Every interactive widget maps explicit state to a pure view:

`idle → focused → validating/loading → submitted | cancelled | unavailable`

- Validation keeps the value and focus in place and gives one actionable correction.
- Loading keeps cancellation available and updates a factual progress label.
- Submission collapses to the question and a concise result.
- Terminal help names the actual outcome (`submitted`, `back`, `cancelled`, `timed out`, or `unavailable`).
- Cancellation is neutral: neither an error nor a success.
- Unavailable interactive UI produces an inline, machine-legible fallback for the agent.

Outcome words and glyphs accompany color: `✓ done`, `⚠ blocked`, `✗ failed`, `running`, and `cancelled`. The TUI honors terminal theme capabilities and `NO_COLOR` through the shared semantic palette and ANSI fallback.

## Core flows

### Ordinary decision

`agent question → inline decision owns focus → user selects, filters, or writes → compact result enters transcript → editor regains focus`

### Consequential plan

`RFC revision ready → terminal summary with revision, paths, and actions → optional browser review → choose Start implementation or Request changes once → Start authorizes the exact bytes, materializes the shared graph, and claims the first dependency-ready step`

The browser plan uses a left-aligned document layout without surrounding cards.
During review, the proposal and decision controls precede the task checklist.
During execution, the checklist precedes feedback. Workflow guidance, dependency
diagrams, and Markdown remain available in expandable sections. Task details
retain declared paths, acceptance criteria, and verification commands.

Pending input pauses execution guidance. Completed, failed, and abandoned plans
report their outcome instead of asking to Start again. Shared task status drives
dependency readiness, so a failed prerequisite remains blocked in every plan
projection even if a local snapshot previously marked it done.

### Long-running work

`tool starts → visible response within 100 ms when possible → one running indicator plus factual updates → terminal outcome row → notify only if attention moved elsewhere`

### Noninteractive host

`interactive UI unavailable → do not open a picker or browser → create a durable authorization interaction when the host supports continuations → return its correlation and exact reviewed revision → after the host records the human answer, resume Start with that revision and interaction ID; otherwise show the explicit plan commands and never infer approval from prose`

### Destructive or external mutation

`show target and consequence → offer safe cancel → require explicit confirmation proportional to risk → mutate once → show receipt and recovery path`

## Evidence behind the contract

- The [Command Line Interface Guidelines](https://clig.dev/) require TTY-gated prompts, a noninteractive alternative, a clear escape route, early feedback, useful progress, and concise human-readable errors.
- [Inquirer select guidance](https://github.com/SBoudrias/Inquirer.js/blob/main/packages/select/README.md) uses pagination for lists longer than seven items, focused descriptions, visible disabled reasons, configurable key help, and stable default focus.
- [Bubble Tea](https://github.com/charmbracelet/bubbletea) and [Ratatui's Elm architecture guidance](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/) keep state updates separate from pure rendering. [Ratatui layout guidance](https://ratatui.rs/concepts/layout/) supports constraint-based adaptation to terminal size.
- WCAG's keyboard principles require [logical focus order](https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html) and a perceivable focus indicator. Its [use-of-color guidance](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color) requires text, shape, or another cue in addition to color.
- The [`NO_COLOR` convention](https://no-color.org/) and terminal capability checks keep output usable in monochrome and automated environments.

## Conformance checklist

A new or changed widget is not complete until it satisfies these checks:

- One owner for each displayed fact and one keyboard-focus owner.
- Pure width-bounded render at 36, 52, 80, 120, and 160 columns.
- Logical keyboard order, visible focus, Enter, Escape/Ctrl-C, and current-state help.
- Text or glyph state cues that remain understandable with color removed.
- Empty, loading, disabled, validation, success, failure, cancellation, and unavailable states.
- No prompt or automatic browser launch without interactive user choice.
- Targeted renderer/input tests, typecheck, package build, and a real TTY smoke for changed interaction paths.

## Configuration

Run `/configuration`, also shown in the footer, to open the local browser controls.
The extension adds one slash command. Workflow choices remain with the user and
model; input text and model output do not trigger regex-generated instructions.

The page includes session permissions, footer density, theme, effort, MCP
connections/tools, skill enablement, and the live command inventory. Review plan
opens a live page whose Start and Request changes buttons apply typed actions.
Browser feedback goes to the agent as plain text, without command expansion.

The runtime tool inventory remains:

```text
✓ tools: 0 native Pi tools + 16 support tools
```

## Command inventory

The browser reads `pi.getCommands()` when opened. It hides private names, groups
entries by source, and supports search. Octocode contributes `/configuration`;
host commands and user-installed skills or prompts keep their own entries.

At startup the footer checks GitHub login through `npx octocode auth status --json`.
It retains status only; credentials are never rendered.

Scrollback rule (pi-tui `tui-main-screen.js`): a change to any line **above the visible viewport** — or a width/height change — forces a full redraw that clears the screen *and scrollback*. Octocode therefore renders **nothing above the transcript**: it does not call `setHeader`, and the session name lives only in the terminal title. Every transcript entry, message, and tool row is a pure function of its data. Live operational state stays in the register-once footer; unrelated mode/config chips remain status-owned. Repaints use `tui.requestRender`, and per-frame render closures never do O(session) work. Events and the 1-second tick sample context usage because `pi.getContextUsage()` rebuilds the session branch per call; the banner also memoizes its version read. Diagnose any remaining full redraw with `PI_DEBUG_REDRAW=1` (pi logs each `fullRender:` reason to `pi-debug.log`).

Motion language: the transcript and footer use no animated decoration — pi's working spinner is the only moving glyph; live agent rows only update factual elapsed/state/message text. The palette paints attention flags (`⚠ ✗ ✉`, ≥90% context) as warning/error **and bold**; brightness always means state, never decoration. The banner card is a fixed purple gradient — an animated banner at the top of the scrollback invalidated pi-tui's line diff on every repaint and caused scroll jumps.

## Agent ledger

The footer shows current workers. Ask the agent to inspect, message, wait for, or
stop a worker through its `agent` tool. Notifications remain available without
an extension slash command.

Ledger badges:

| Badge | Meaning | User action |
|---|---|---|
| `⚠ recovery` | Evidence-free status/action loop detected | Inspect, re-diagnose, verify independently |
| `⚠ needs verify` | Done handback lacks evidence/verification | Run acceptance checks before final answer |
| `blocked` | Worker asked parent for input | Send an answer with an `agent` message query |
| `failed` | Process/tool failed | Inspect stderr/output. Retry or kill. |
| `msg→ <action>` | Parent sent, steered, or queued a message to this worker | Watch the queued count or wait for the turn |
| `msg← reply` | Worker replied to the parent | Read the preview or run an `agent` inspect query |

## Visual contract

`src/tui/cli-design.ts` owns the Pi adapter's visual language: core glyphs, spinner frames, transcript tool rows, activity rows, compact payload summaries, and raw ANSI fallback colors. Pi component renderers still use `src/tools/render-helpers.ts` for width-safe output, but they import symbols and progress primitives from this contract so extension surfaces do not drift. Activity can report that a model is working; it never renders or stores private reasoning text.

## Color system

`src/tui/palette.ts` (`TOKEN`) is the only place a *kind of data* is bound to a theme token; `themes/octocode-{dark,light}.json` own the hex values. Every surface — banner, header, footer, plan panel, tool rows, ask-user, agent ledger, overlays — paints through `paint(theme, token, …)` so one colour keeps one meaning everywhere:

| Colour | Tokens | Means | Never used for |
|---|---|---|---|
| **Purple** (`accent`) | `brand`, `title` | Octocode identity: the `◆` mark, banner body, tool names, the focused/selected row, and anything **in flight** (spinner, `running`, `doing`, `Fetching…`, `Spawning agent…`) | warnings, success |
| **Lavender** (`mdLink`) | `link` | Links and peer/agent messaging (`✉` unread, `queued` workers) | decoration |
| **Sky** (`mdCode`) | `path`, `symbol` | File paths and identifiers — the data you read most; distinct from purple so a path never looks like a tool title | — |
| **Gold** (`warning`) | `warning` | **Act on me**: blocked workers `⚠`, `perm relaxed`, ≥75 % context, genuine tool warnings | frames, spinners, in-flight labels, "no match", cancels, pros/cons |
| **Green / Red** | `success`, `error`, `diffAdd`, `diffRemove` | Outcomes only: done/failed rows, `✓`/`✗` result glyphs, `+`/`-` diff lines | selection state, recommended badges |
| **Default fg** | `count`, `bright` | Values such as counts and totals, plus pending plan rows — bright against dim labels | — |
| **Grey ramp** | `muted` → `dim` → theme `faint` | Secondary text → chrome (separators, `│` bars, hints, finished plan rows) → rules | primary content |

The footer speaks in words, not glyphs, across responsive rows: `ctx ▓▓░░ 25% (250k/1M) · plan 3/6 · task 4 validating UI`, followed by identity/configuration and one row for each visible worker. When width is limited, each row keeps its highest-priority content and truncates safely; it does not hide footer state behind `+N`. `/octocode-status`, `/octocode-agents`, and `/octocode-harness` retain diagnostic detail.

Attention states in the footer (`⚠`, `✗`, `✉`, near-full ctx) are additionally **bold** (`FooterSegment.attention`) — the only emphasis in the toolbar, so bold always means "look here". Per-row budget: at most three colours plus the grey ramp.

Raw ANSI output (shell transcript rows, `coloredDiff`) goes through `cli-design.ts`'s `ansiForToken` fallback map, which mirrors the theme mapping (`linkUrl` → dim, `bright` → bold) and honours `NO_COLOR`.

## Width and theme rules

- Shared width-safe renderers build every rendered line.
- Use theme colors from callback contexts when Pi provides a theme; raw shell rows use the visual contract's `NO_COLOR`-aware fallback.
- The `askUser` decision picker uses Pi `ctx.ui.custom(builder)` inline (no overlay options) so the prompt appears in the message flow at the bottom, reading as part of the conversation rather than a floating overlay box.
- Footer/status success is quiet; warnings and errors notify.
- Keep widgets compact; use commands for detailed output.

## Troubleshooting

| Symptom | Action |
|---|---|
| Extension looks inactive | Run `/octocode`, then `/octocode-harness` |
| Ledger is noisy | Run `/octocode-agents hide` or `/octocode-agents prune` |
| Context bar is near full | Compact or use `/octocode-harness` for prompt-overhead details |
| Worker says done too early | Inspect and verify acceptance yourself |
