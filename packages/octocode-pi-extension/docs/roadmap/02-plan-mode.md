# 02 — Plan mode

**Status:** Proposed · **Priority:** P1 · **Depends on:** [01-permissions.md](01-permissions.md) M1 for the shared engine (plan M1 can ship with its own small gate) · **Owner area:** `src/permissions/plan.ts` (new), `src/turn.ts`, `subagents/plan.md`

## Problem and evidence

The user cannot tell the main session "look and propose, change nothing".

| Today | Why it is not plan mode |
|---|---|
| Ask in words ("don't edit") | Prompt only; the model can still call `file` and bash |
| `/octocode review on` (`src/files/review.ts:74-90`) | Asks per `file` batch; bash, browser and MCP still act; no planning flow |
| Delegate to `researcher` (`subagents/researcher.md:4`) | Excludes `file,browser`, but bash is read-only by instruction only; the plan returns as a report |
| Pi `examples/extensions/plan-mode` | A demo loaded only with `-e`; regex allowlist tests only the first word (`utils.ts:97-100`), so `ls && rm -rf src` passes; it rewrites the active tool set |

The prompt already has one read-only case: an agent without `file`/`write`/`edit` gets "investigate and report; do not modify files" (`src/prompt.ts:61,73`). That rule depends on the active tools, which plan mode must not change.

## Competitor research

| Product | Enter | Blocked | Shell in plan | Exit |
|---|---|---|---|---|
| Claude Code | Shift+Tab cycle; `/plan`; `defaultMode: plan` | Edits | Allowed "to explore" | Dialog: approve + auto, approve + manual edits, keep planning |
| Codex CLI | `sandbox_mode = "read-only"` | All writes (Seatbelt/Landlock) | Allowed; writes fail at the OS | Change sandbox mode |
| OpenCode | Tab to `plan` agent | Edits and bash set to `ask` | Ask | Tab back to `build` |

Sources:

- Claude Code <https://code.claude.com/docs/en/permission-modes>: plan mode "reads files, runs shell commands to explore, and writes a plan, but does not edit your source"; "Approving a plan exits plan mode"; blocks hold "including non-interactive runs with -p"; but with bypass permissions available it "doesn't enforce plan mode's blocks" (avoid).
- Codex <https://github.com/openai/codex> at `822e58cc`: `codex-rs/protocol/src/config_types.rs:104`; `codex-rs/protocol/src/protocol.rs:1058-1100`; `codex-rs/sandboxing/src/seatbelt.rs:21-28`.
- OpenCode <https://opencode.ai/docs/agents/>: Plan is "A restricted agent designed for planning and analysis … all of the following are set to ask: file edits … bash".
- Pi example: `examples/extensions/plan-mode/index.ts:53` (flag), `:141` (command), `:158` (shortcut), `:201-223` (message injection).

**Copy:** one key toggles it and the footer shows it; enforce in code and keep blocks headless; allow read-only shell; end with an explicit choice; a read-only `plan` agent for delegation.

**Avoid:** first-word allowlists (check every segment with the shared scanner); an escape hatch that disables the blocks; changing the tool list per mode (breaks the prompt cache); calling plan a sandbox (scripts behind `yarn test` can write, so they ask).

## Pi API constraints

| Need | Pi API | Note |
|---|---|---|
| Toggle by key | `pi.registerShortcut` (`dist/core/extensions/types.d.ts:1190-1193`) | Shift+Tab is reserved (`app.thinking.cycle`, `dist/core/extensions/runner.js:9-27`). Use **Ctrl+Alt+P** |
| Slash command | `Subcommands` + alias list `SHORTCUTS` (`src/index.ts:48`) | `/octocode plan` and `/plan` |
| Start flag | `pi.registerFlag('plan', { type: 'boolean' })` | Also `OCTOCODE_PLAN=1` |
| Block calls | `tool_call` → `{ block, reason }` | Same gate chain as 01 |
| Tell the model | `before_agent_start` returns `message` | Appended to the conversation; the cached prefix holds |
| Keep the cache | Do **not** call `pi.setActiveTools` per mode | Tools set once (`src/turn.ts:47-60`); section rebuilt only on input change (`src/turn.ts:84-88`) |
| Persist | `pi.appendEntry()` + read the branch on `session_start` (Pi `docs/extensions.md:225-229`) | Survives `/resume` and reload |
| Exit dialog | `agent_end` + `ctx.ui.select`; `pi.sendUserMessage` | Only with `ctx.hasUI` |

**Names.** Octocode owns `--plan`, `/plan` and Ctrl+Alt+P: the same three the Pi example registers. The example is a demo loaded only with `-e`; Octocode's plan mode replaces it. No rename. `docs/CONFIGURATION.md` says not to load both; Pi reports the conflict if a user does. We do not copy the example; we reuse its key and its message-injection technique.

## Design

### State and modes

Plan is the third mode from 01: `default | auto | plan`. State is `{ mode, previous }`; leaving plan restores `previous`. Every change is stored with `pi.appendEntry('octocode-permission-mode', { mode })` and restored from the branch on `session_start`.

| How | Effect |
|---|---|
| `/octocode plan [on\|off]`, `/plan` (no argument toggles) | Set mode |
| Ctrl+Alt+P | Toggle |
| `pi --plan`, `OCTOCODE_PLAN=1` | Start in plan |
| `/octocode permissions mode plan` | Same as `/plan on` |
| Footer | `plan` in accent via `ctx.ui.setStatus('octocode-plan', …)` (pattern `src/files/review.ts:76-79`) |

A project file cannot set any mode (UX rule; plan only restricts).

### The plan preset

A fixed rule set evaluated **before** user rules. A plan `deny` is final; a plan `allow` still passes user rules (a user `deny` holds); a plan `ask` follows 01 (deny without UI).

| Tool | Plan decision |
|---|---|
| `read`, `grep`, `find`, `ls` | allow |
| `mcp__octocode__*` (all nine are read-only) | allow |
| `web` | allow |
| `file`, `edit`, `write` | **deny**, except under `<workspace>/.octocode/tmp/plans/**` (gitignored, `.octocode/tmp/.gitignore`) for long plans |
| `bash` | allow when **every** segment is on the read-only list with no write redirection; opaque → deny; else → ask |
| `browser` | allow `info`, `navigate`, `snapshot`, `screenshot`, `console`, `tabs`, `tab`, `wait`, `hover`; ask `click`, `type`, `fill`, `press`, `drag`, `dialog`, `evaluate`; deny `upload`, `download` |
| `agent` | allow `researcher`, `reviewer`, `plan`, `webHeadless`; deny others and the general worker; children run in plan |
| `askUser`, `coordinate`, `sendMessage`, `memory`, `backlog` | allow (session state, not code) |
| other MCP / unknown | ask |

Read-only bash list (`src/permissions/plan.ts`, strict token form from 01):

| Group | Commands |
|---|---|
| Files | `ls`, `cat`, `head`, `tail`, `wc`, `stat`, `file`, `tree`, `du`, `df`, `pwd`, `realpath`, `basename`, `dirname` |
| Search | `rg`, `grep`, `ag`, `fd`, `find` (not `-exec`, `-execdir`, `-delete`, `-fprint*`, `-ok`) |
| Text | `sort`, `uniq`, `cut`, `tr`, `jq`, `diff`, `cmp`, `column`, `echo`, `printf`, `sed` (not `-i`/`--in-place`) |
| Git | `git status`, `log`, `show`, `diff`, `blame`, `grep`, `ls-files`, `ls-tree`, `rev-parse`, `describe`, `shortlog`, `cat-file`, `branch` (no `-d`, `-D`, `-m`, `-M`, `-c`), `remote -v`, `tag -l`, `stash list`, `config --get`; never a global `-c` |
| Info | `which`, `type`, `command -v`, `env` (no arguments), `uname`, `date`, `node -v`, `npm ls`, `npm view`, `yarn why`, `npx octocode` |

Redirection `>`, `>>`, `&>`, `>|` or `tee` to anything but `/dev/null` → deny. `<` is fine. `background: true` → ask.

The list is code in M1. In M4 the global policy can add mode-scoped allows, which turn a plan `ask` into allow, never a plan `deny`:

```json
{ "tool": "bash", "command": "yarn test*", "action": "allow", "modes": ["plan"] }
```

### Tool set and prompt (cache)

- The active tool set does not change. `file` stays visible; its calls are blocked. The tool block and `octocode` section stay byte-identical, so the prompt cache holds and `canWrite` (`src/turn.ts:72`) is stable. `src/prompt.ts` does not change; `loadProfiles` lists `plan`.
- First turn after entering: `before_agent_start` returns one message (`customType: 'octocode-plan'`, `display: false`):

  > Plan mode is on. Investigate and propose; do not change files or state. File changes, write commands and other side effects are blocked; read-only shell, Octocode research, web and read-only subagents (`researcher`, `reviewer`, `plan`, `webHeadless`) work. End with a section `## Plan` holding numbered steps (files, change, verification), risks and open questions. Save a long plan under `.octocode/tmp/plans/` and name the path.

- First turn after leaving: "Plan mode is off. You may change files now; follow the agreed plan."
- After `session_compact` in plan mode, the enter message is sent again. No message on other turns.
- Block reason: `Plan mode: <tool/command> would change state. Investigate read-only, or ask the user to leave plan mode (/plan).`

### Exit flow

On `agent_end` in plan mode with a UI, when the last assistant message has `## Plan` (or names a `.octocode/tmp/plans/` file), show `ctx.ui.select('Plan ready', …)`:

| Option | Effect |
|---|---|
| Execute | Mode → `previous`; `pi.sendUserMessage('Execute the plan above.')` |
| Execute with review | Same, plus file review on (`src/files/review.ts`, `mode.on = true`) |
| Save to backlog | Backlog item with the plan (`src/backlog/store.ts`); stay in plan |
| Keep planning | Close; stay in plan |

Dismiss = keep planning. Headless: no dialog; `pi -p --plan` returns the plan as its answer.

### `plan` subagent profile

New bundled `subagents/plan.md`:

```markdown
---
name: plan
description: Read-only planner. Investigates the code and returns a numbered implementation plan with files, steps, checks, risks and open questions. Use before a non-trivial change, or in parallel for alternative plans.
excludeTools: file,browser,askUser
permissions:
  mode: plan
---
Produce a plan the parent can execute without redoing your research. …
```

- A profile may set `permissions.mode: plan` (it only restricts); the loader rejects `auto` (01).
- The child gets the preset through `OCTOCODE_PERMISSIONS_POLICY` (01). Its `ask` becomes deny (no UI); it reports the gap.
- No `file` tool, so the plan goes in the report; over 8 KB it spills to `report.md` in its scratch folder (FEATURES.md, subagents). The `.octocode/tmp/plans/**` exception applies only to a main session.
- Fan-out ([05](05-fan-out-batches.md)) can start N `plan` children for alternative plans.
- A forked child (`context: 'fork'`, [03](03-agent-wait-and-context.md)) of a plan-mode parent starts in plan; mode is part of the policy, not the transcript.
- M3 adds `permissions.mode: plan` to `researcher` and `reviewer`; their prompts keep the read-only instruction.

### Interactions

| With | Rule |
|---|---|
| User rules (01) | Plan preset first; plan deny is final; user deny still applies to plan allows |
| `auto` | Leaving plan returns to `auto` if that was `previous`; plan `ask` is **not** auto-approved |
| File review | Independent; "Execute with review" turns it on |
| Gate order | Reservation, catastrophic, permissions (plan inside), hooks |
| User `!cmd` | Not restricted |
| Checkpoints | None, since no `file` batch runs |
| Running subagents | Keep their policy; plan applies to new spawns; `/agents` ([04](04-agents-view.md)) shows each child's mode |

### Failure modes

| Case | Behavior |
|---|---|
| Model retries blocked calls | 01 loop guard: 3 identical blocks in a turn → `terminate: true` |
| Shortcut skipped (conflict, e.g. the Pi example also loaded) | Pi reports a diagnostic; `/plan` still works |
| Corrupt restore entry | Start in `default`, warn |
| Gate error | Block (fail closed) |
| Side effect through an allowed tool (`npm view` hooks, a `navigate` GET with effects) | Accepted residual risk; documented |

### Security notes

- A guard against accidental edits, not a sandbox; shell analysis has the limits in 01.
- Interpreters (`node -e`, `python -c`), package scripts (`yarn test`) and build tools are off the list on purpose; they ask.
- Exit dialog text passes `sanitizeTerminalText`.

## Files to change

| Path | Change |
|---|---|
| `src/permissions/plan.ts` (new) | Preset, read-only bash list, redirection check, browser table |
| `src/permissions/gate.ts` (01) | Plan preset first in `plan` mode |
| `src/permissions/mode.ts` (new) | Mode state, `appendEntry` persistence, restore, footer, flag/env |
| `src/permissions/command.ts` (01) | `/octocode plan`, mode switch |
| `src/index.ts` | `plan` in `SHORTCUTS` (line 48); Ctrl+Alt+P shortcut; `plan` flag |
| `src/turn.ts` | Enter/leave `message` from `before_agent_start`; no tool or section change |
| `src/files/bash-guard.ts` | `commandSegments` export (shared with 01) |
| `src/subagents/profiles.ts`, `process.ts` | `permissions.mode`; child policy env |
| `subagents/plan.md` (new); `researcher.md`, `reviewer.md` | New profile; `permissions.mode: plan` |
| `src/backlog/store.ts` | Reused for "Save to backlog" (no change expected) |
| `docs/FEATURES.md`, `docs/CONFIGURATION.md`, `README.md` | Plan mode, profile row, `OCTOCODE_PLAN`, commands, shortcut; CONFIGURATION: do not load Pi's plan-mode example with Octocode |

## Phased plan

| Milestone | Scope | Ships |
|---|---|---|
| M1 Toggle + hard blocks | Mode state, `/plan`, Ctrl+Alt+P, `--plan`, footer, persistence; `file`/`edit`/`write` deny; segment-checked read-only bash; messages. Own gate if 01 is not merged | Main-session plan mode |
| M2 Full preset | Browser table, `agent` filter, MCP/unknown ask, redirection check, loop guard | Full tool coverage |
| M3 Profiles | `subagents/plan.md`; `permissions.mode: plan` for `plan`, `researcher`, `reviewer`; child inheritance | Enforced read-only delegation |
| M4 Exit flow | Dialog: execute / with review / save to backlog / keep planning; mode-scoped user allows | Plan → execute loop |

## Test plan

**Unit:**

- Preset: each tool row, each browser action, `agent` with each profile.
- Bash: `git log`, `rg x | head`, `ls && cat a`, `cat a > /dev/null` → allow; `ls && rm -rf src`, `echo x > a`, `sed -i`, `find . -delete`, `git -c core.pager=x log`, `bash -c 'ls'`, `node -e 1` → deny; `yarn test` → ask.
- `file` under `.octocode/tmp/plans/` allowed; elsewhere, `..` and symlink escape denied.
- Mode: toggle restores `previous`; restore from branch; corrupt entry → default.
- Messages: once on enter, once on leave, again after compaction; same `promptKey` in and out of plan.
- `pi.setActiveTools` is not called on toggle.

**End-to-end (scripted model, real Pi session):**

- `--plan`: `file` write blocked with the plan reason, file absent; `bash git status` runs.
- Headless `ask` (`yarn test`) → denied with reason.
- `agent` `implementer` in plan → refused; `plan` → child runs, its `bash touch x` is blocked.
- Exit flow via RPC extension UI: "Execute" leaves plan and sends the follow-up.

**Real Pi flow:** `yarn build`; `pi --no-extensions -e packages/octocode-pi-extension/dist/index.js -e builtin:mcp -e builtin:tool-search`; press Ctrl+Alt+P and check the footer; ask for a refactor; watch blocked edits and allowed research; choose "Execute with review"; `/resume` and check the mode; run `agent` with profile `plan`.

## Open questions

1. Non-listed bash in the interactive main session: `ask` or `deny`? Proposed: `ask` (Claude explores, OpenCode asks). Headless stays `deny`.
2. Allow `memory` writes in plan? Proposed: yes; memory is the user's store, not the repository.
3. `/plan <text>` (Claude's one-prompt form)? Proposed: M4; turns plan on and sends the text.

## Out of scope

A plan progress tracker (the example's `[DONE:n]`; backlog covers it); an OS read-only sandbox (Codex style); a separate "acceptEdits" mode (file review plus 01 cover it); changing the active tool set per mode.
