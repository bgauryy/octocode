# Octocode Awareness

`octocode-awareness` gives the agent awareness. It lets one agent know what has happened in a workspace, what files are being touched, what another run already learned, what still needs verification, and what should be handed to the next agent.

The skill is especially useful when several agents work together in the same repo, even when those agents come from different vendors or hosts. They do not need to share raw chat logs to coordinate; they share a local awareness layer.

## The Problem

Coding agents are usually stateless between runs. One agent may edit a file while another is reading stale context. A later run may rediscover a lesson that was already learned. A handoff can be buried in chat, and a success claim can be made without a recorded check.

`octocode-awareness` turns that invisible state into local, inspectable coordination data. It is not a search engine or test runner. It is the memory, lock, handoff, notification, and verification layer around engineering work.

## Capabilities

- Shared memory for reusable lessons, failure signatures, decisions, and gotchas.
- Workspace and branch-scoped handoffs for unfinished or ongoing work.
- File claims so agents can see overlapping edits before they collide.
- Verification records that connect a work intent to the check that actually ran.
- Agent-to-agent notifications for blockers, questions, claims, replies, and handoffs.
- Subagent receipts that preserve scope, sources, and decision impact without storing raw chat logs.
- Reflection and weakness-mining flows that turn repeated failures into better future behavior.
- Reasoned self-harness proposals for `AGENTS.md`, docs, standing memory-corpus changes, and the skill code itself, always behind user approval before edits.
- Optional local semantic recall while keeping SQLite and text search as the dependable default.
- A local viewer for inspecting memories, locks, intents, refinements, and notifications.

## Operating Model

The skill uses a shared local SQLite store under the user's Octocode state directory. Records are scoped by workspace, repo, branch/ref, file path, state, and agent id, so the same memory layer can support multiple projects without needing a separate database per repo.

The mental model is:

```text
ATTEND -> FOCUS -> CLAIM -> WORK -> VERIFY -> ENCODE -> SLEEP
```

An agent attends to the current state, focuses the intended work, claims files when editing, does the work, records verification, encodes reusable lessons or repo handoffs, then leaves the workspace clean for the next run.

Hooks can automate parts of this lifecycle in hosts that support them. Manual use still works everywhere, which is what makes the skill portable across agents and vendors.

## How Users Use It

After installation, ask your coding agent to use `octocode-awareness` before it edits a repo.

From there, the agent should make the awareness layer visible in plain language:

- what previous runs learned that may matter,
- which files are already claimed,
- what handoffs or unread messages exist,
- what verification is still owed,
- what it saved for the next run.

If automatic hooks are available in your agent host, they can enforce parts of this flow. Otherwise, the agent can call the bundled scripts manually. The exact commands live in `SKILL.md`, `references/`, and `scripts/` because those files are for agents and maintainers, not for the user-facing overview.

## Storage And Semantic Recall

Awareness uses one local SQLite database under Octocode's state directory by default. It can also export important memories into a repo so a team can share them through normal code review. Semantic recall is optional and local; it does not require a separate semantic database or external service beyond the first model download when indexing is enabled.

## User Experience

For users, the value is less drama in shared workspaces. The agent can say which files are claimed, what remains unverified, what a previous run learned, and what handoff is waiting. The user gets a clearer answer to "what is going on here?" before another agent starts editing.

The skill also makes collaboration more honest. A conclusion can carry a recorded verification trail, and a future agent can distinguish "someone thought about this" from "someone proved this."

When repeated evidence shows the harness itself should improve, the agent may propose updates to project instructions, docs, standing memory guidance, or the awareness skill. Those proposals must explain why the change is needed and wait for user approval before files or standing memories are changed.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-awareness
```

Optional hooks can make awareness more automatic. Users can start with manual coordination and add host-specific automation later.

## Maintainer Notes

Keep this README user-facing: what awareness solves, what users can expect from their agent, how to install it, and the high-level storage/privacy model. Keep operational commands, flags, schemas, and protocol details in `SKILL.md`, focused references, and scripts.
