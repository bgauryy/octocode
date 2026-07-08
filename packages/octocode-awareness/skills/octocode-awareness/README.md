# Octocode Awareness

`octocode-awareness` gives the agent live workspace awareness. It lets one agent know what has happened in a workspace, what files are being touched, what another run already learned, what still needs verification, and what should be handed to the next agent.

The skill is especially useful when several agents work together in the same repo, even when those agents come from different vendors or hosts. They do not need to share raw chat logs to coordinate; they share a local awareness layer.

## The Problem

Coding agents are usually stateless between runs. One agent may edit a file while another is reading stale context. A later run may rediscover a lesson that was already learned. A handoff can be buried in chat, and a success claim can be made without a recorded check.

`octocode-awareness` turns that invisible state into local, inspectable coordination data. It is not a search engine or test runner. It is the single CLI-oriented skill for memory recall, file locks, handoffs, signals, verification, reflection, hooks, repo context, and learning around engineering work.

## Why Agents Use It

Awareness is the agent's situational layer:

- **Before work**: inspect repo status, other agents, active locks, pending verification, durable memories, gotchas, refinements, signals, and generated wiki context.
- **During work**: think with current context, lock files before edits, communicate conflicts or decisions, record durable facts, and avoid clobbering another agent.
- **After work**: run and record verification, reflect on lessons, update memories, refresh wiki projections when useful, and leave handoffs or cleanup state visible.
- **Ongoing**: use housekeeping to prune stale state, and use skill/workflow updates when repeated patterns deserve automation.

## Capabilities

- Scoped recall for reusable lessons, failure signatures, decisions, and gotchas.
- Workspace and branch-scoped handoffs for unfinished or ongoing work.
- File claims so agents can see overlapping edits before they collide.
- Verification records that connect an edit task to the check that actually ran.
- Agent-to-agent signals for blockers, questions, claims, replies, and handoffs.
- Session capture and refinements that preserve scope, decisions, and handoffs without storing raw chat logs.
- Reflection records for durable lessons, failure signatures, cleanup decisions, and staged harness improvements.
- A local view of active memories, locks, tasks, refinements, and signals.
- Optional workspace `.octocode/` repo context projections that act like a generated LLM Wiki over the awareness store.
- Housekeeping commands for stale locks, redundant memories, old signals, refinements, and docs drift.
- A path from repeated failures to better skills: mine weaknesses, export guidance candidates, then update skills with `octocode-skills` or the `npx octocode` CLI.

## Operating Model

The skill uses a shared local SQLite store under the user's global Octocode home, normally `~/.octocode/memory/awareness.sqlite3`. Workspace path is the primary scope key, with optional artifact/package/service, repo, branch/ref, file path, state, and agent id filters layered under it, so the same memory layer can support multiple projects without needing a separate database per repo.

### State machine (skill + CLI + hooks)

```text
                    ┌─ notify-deliver / sessionStart (hook) ─┐
                    ▼                                         │
 IDLE ──attend──▶ ATTEND ──lock acquire / pre-edit──▶ CLAIMED
   ▲                 │                                  │
   │                 │ memory/signal/refine             │ edit (agent)
   │                 ▼                                  ▼
   │              LEARN ◀──────────────────── PENDING_VERIFY
   │                 ▲                         ▲     │
   │                 │                         │     │ verify mark
   │                 │                    post-edit  │
   │                 │                         │     ▼
   │            REFLECT ◀── reflect record ── VERIFIED
   │                 │
   │                 ├── session capture / signal handoff ──▶ HAND_OFF
   │                 └── repo inject ──▶ PROJECTED (.octocode wiki)
   │                                        │
   └──────── digest / prune ◀───────────────┘
```

| State | Skill phase | CLI | Hook reflex |
|---|---|---|---|
| ATTEND | Before | `attend`, `query workboard`, `signal list` | `notify-deliver` / sessionStart briefing |
| CLAIMED | During | `lock acquire` | `pre-edit.sh` |
| PENDING_VERIFY | After edit | (task pending) | `post-edit.sh` |
| VERIFIED | After | `verify mark` / `verify audit` | `stop-verify.sh` blocks silent conclude |
| REFLECT / LEARN | After | `reflect record`, `memory record` | — |
| PROJECTED | After | `repo inject` → `.octocode/AGENTS.md` + wiki | — |
| HAND_OFF | After / end | `session capture`, `refinement *`, `signal *` | `session-end.sh` |
| IDLE / clean | Ongoing | `maintenance digest --dry-run`, prune | optional digest on notify |

Manual CLI works without hooks. Hooks only automate the same transitions. Details: `references/full-flow.md`, `references/hooks.md`.

### Wiki / memory projections

`query <view>` reads the live store. `repo inject` regenerates `<repo>/.octocode/` with `.octocode/AGENTS.md` (digested map), `MEMORY.md`, `GOTCHAS.md`, `LEARN.md`, `BOOKMARKS.md`, CSV, HTML, and references. Agents should keep a short pointer in root `AGENTS.md` → `.octocode/AGENTS.md` so default loaders find the map (see `references/repo-context-management.md`).

## How Users Use It

After installation, ask your coding agent to use `octocode-awareness` before it edits a repo.

From there, the agent should make the awareness layer visible in plain language:

- what previous runs learned that may matter,
- which files are already claimed,
- what handoffs or unread messages exist,
- what verification is still owed,
- what it saved for the next run.

If automatic hooks are available in your agent host, they can enforce parts of this flow. Otherwise, the agent should call `node scripts/awareness.mjs` from the bundled skill folder, or the repo-local `dist/bin/awareness.js` when working in this package; use `npx @octocodeai/octocode-awareness` only when no local CLI exists. The exact commands live in `SKILL.md`, `references/`, and `scripts/` because those files are for agents and maintainers, not for the user-facing overview. Start technical onboarding with `references/full-flow.md`. Older prompts that name `octocode-reflection` or `octocode-agent-communication` should load this skill.

When the agent sees repeated workflow friction, ask it to improve the workflow rather than only recording another note. It should use `octocode-skills` when installed, or `npx octocode` to install/manage skills; the user-facing Octocode guide starts at `https://octocode.ai`.

## Storage And Recall

Awareness uses one local SQLite database under the global Octocode home by default. It can also export repo-scoped memory projections into `<repo>/.octocode/` so a team can share them through normal code review. The bundled Node runtime uses SQLite FTS, scope filters, references, and smart lexical broadening. Older Python-only semantic indexing notes are reference material, not a feature of the shipped `awareness.mjs` runtime.

## User Experience

For users, the value is less drama in shared workspaces. The agent can say which files are claimed, what remains unverified, what a previous run learned, and what handoff is waiting. The user gets a clearer answer to "what is going on here?" before another agent starts editing.

The skill also makes collaboration more honest. A conclusion can carry a recorded verification trail, and a future agent can distinguish "someone thought about this" from "someone proved this."


## Installation

Install the bundled skill with:

```bash
npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common
```

The path form is for agents that already know where bundled skills live. They provide that local folder path, and the Octocode CLI installs from it without the awareness package trying to infer host-specific skill locations. A registry fallback is `npx octocode skill --name octocode-awareness`.

Optional hooks can make awareness more automatic. Users can start with manual coordination and add host-specific automation later.

Hooks are the strongest way to make awareness operations reliable for agents: pre-edit hooks claim files, post-edit hooks mark work as pending verification, stop hooks prevent silent unverified conclusions, and prompt/session hooks deliver context and capture handoffs.

## Maintainer Notes

Keep this README user-facing: what awareness solves, what users can expect from their agent, how to install it, and the high-level storage/privacy model. Keep operational commands, flags, schemas, and protocol details in `SKILL.md`, focused references, and scripts.
