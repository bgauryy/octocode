# @octocodeai/pi-extension

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-pi-extension/assets/logo.png" width="640px" alt="Octocode + Pi">
</div>

> The research, memory, and operating model layer for Pi coding agents — powered by a bundled Octocode CLI.

---

## Motivation

**One leading tool for everything the agent needs to understand — local *and* external.** Instead of a scatter of `grep`/`find`/`cat`/`gh`/`npm`, the agent reaches for a single research surface: locally it reads files, trees, symbols, and structure with full-text search, LSP semantics, and AST matching (token-lean `symbols → compact → exact` reads); externally it researches GitHub code, PRs, history, repos, and npm packages through the same interface. Local clues feed external prior-art search and external findings feed exact local reads — one tool, both directions.

**Quality comes from the combination, not any single piece:**

- **Bundled Octocode CLI** — the leading instrument for discovery, on disk the moment `pi install` finishes. No `npx` cold start, no version drift between the CLI, the skills, and the prompt.
- **Skills** — tested research workflows (investigate, review, brainstorm, RFC, memory) the agent activates instead of reinventing a research loop every session.
- **System prompt** — an evidence-first operating model enforced on every turn: search results are leads, proof is an exact read or a passing test, and Octocode leads all discovery.

Each layer alone is partial. Together they make a blank-slate agent research, reason, and verify like a senior engineer by default — which is where the quality comes from.

---

## The problem with blank-slate agents

Pi is intentionally minimal. A lean terminal harness, no opinions, stays out of your way. That's exactly right — until you actually try to use it on a real codebase.

A blank-slate agent will:

- **Grep instead of understand.** It searches for text, not meaning. It finds the string "authenticate" in 40 files and picks one at random.
- **Act on search results as if they were proof.** It sees a function name, assumes it's the right one, and edits it. No call graph. No callers checked. Regressions shipped.
- **Lose everything at compaction.** The context window fills, Pi compresses, and the agent forgets what it decided, what it changed, and why.
- **Reinvent the wheel every session.** Same research loop, no shared patterns, no accumulated workflow knowledge.

These aren't Pi's failures. They're gaps that every coding agent harness leaves open. This package fills them.

---

## What this package adds

Three building blocks, assembled into one `pi install`:

```
┌─────────────────────────────────────────────────────┐
│                      Pi Agent                        │
│                                                      │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────┐ │
│  │ APPEND_SYSTEM│   │    Skills    │   │Extension │ │
│  │  (operating  │   │  (research,  │   │ (locks,  │ │
│  │   model +    │   │  memory,     │   │commands, │ │
│  │   rules)     │   │  review...)  │   │ setup)   │ │
│  └──────────────┘   └──────────────┘   └──────────┘ │
│            │               │                │        │
│            └───────────────┴────────────────┘        │
│                            │                         │
│              Bundled Octocode CLI (dist/bin/)         │
│         (local · GitHub · npm · LSP · AST)           │
└─────────────────────────────────────────────────────┘
```

---

## Building block 1: The operating model

The system prompt (`APPEND_SYSTEM.md`) is injected into every Pi agent turn. It defines how the agent must think before it acts:

```
orient → hypothesize → search/read → prove → act → verify
```

This isn't a list of tips. It's a behavioral contract enforced on every response.

**Why this loop matters:** most agent failures happen between "I found something" and "I changed something". The agent finds a reference, assumes it's authoritative, makes the edit. The loop breaks this. Orient means checking git state and environment first. Hypothesize means naming what you believe before you look. Search/read means gathering actual evidence. **Prove** means accepting only exact file reads, passing tests, or runtime output — not search results. Act. Then verify.

Key rules the model enforces:

- **Verify ground truth before acting** — `git status`, `git log`, manifest files, environment. Never assume the state you expect.
- **Search results are leads, not proof** — a function found in a search result is a hypothesis. An exact file read is evidence.
- **Minimum-path build check** — seven escalation steps before writing new code: YAGNI → reuse existing → stdlib → platform native → installed dep → one-liner → only then write. Most new code requests shouldn't produce new code.
- **Root-cause bug fixes** — grep every caller before touching anything. The fix belongs in the shared function, not the reported call site.
- **Delegate with a context packet** — `pi -p` with the minimum self-contained prompt a fresh agent needs. No session history dumps.
- **Verify before claiming done** — leave one runnable self-check for every non-trivial change.

The agent reads this every turn. The rules stay in scope regardless of how long the session runs.

---

## Building block 2: Octocode as the research backbone

The Octocode CLI ships bundled inside the package (`dist/bin/`). It's ready the moment `pi install` completes — no separate install, no `npx` cold start, no version drift between the CLI and the skills.

The extension injects the exact binary path into the system prompt at session start, so the agent uses `node /path/to/dist/bin/octocode.js` directly.

**Why Octocode instead of grep/find/cat?**

Grep finds strings. Octocode finds meaning.

When a coding agent uses grep, it gets line numbers and text matches. It has no idea whether that function is called from one place or fifty, whether the symbol it found is the canonical definition or a re-export, whether the package it's about to add already exists in the dependency tree.

Octocode gives the agent a research surface that spans everything a real engineer would check:

**Local code — files, symbols, structure, semantics:**

```bash
# Find a term or symbol across a directory
octocode search "authenticate" src/

# Visualize the codebase layout before touching anything
octocode search src/ --tree

# AST pattern: find every arrow function that returns a Promise
octocode search src/ --pattern 'async function $NAME' --lang typescript

# LSP semantics: who calls this function?
octocode search src/auth.ts --op callers --symbol verifyToken --line 42
```

**GitHub — code, PRs, history, repos:**

```bash
# Search across a repo's code
octocode search "rate limit" owner/repo

# Find prior art before building something new
octocode search "webhook signature validation" --target repositories

# Read a PR's context and decisions
octocode search owner/repo#1234 --target pullRequests
```

**npm — packages, APIs, prior art:**

```bash
# Find the right package before adding a dependency
octocode search "jwt validation" --target packages
```

This is the research surface the agent gets by default. The system prompt instructs it to use Octocode for all discovery — before grep, before cat, before any assumption.

---

## Building block 3: Skills — reusable research workflows

Skills are focused workflow prompts that activate on demand. They're loaded automatically into Pi when the package is installed — no setup, no registration, just there.

| Skill | What it does |
|---|---|
| `octocode` | Quick CLI reference and transport lookup |
| `octocode-research` | Full code investigation: local, GitHub, LSP, AST, PR history, architecture |
| `octocode-awareness` | Durable memory: claim files before edits, record decisions, hand off state across sessions |
| `octocode-brainstorming` | Evidence-grounded idea exploration and prior-art research before building |
| `octocode-rfc-generator` | Structured proposals for risky or cross-cutting work before implementation |
| `octocode-roast` | Adversarial code review with severity-ranked findings and fix paths |
| `octocode-skills` | Find, install, rate, and create skills |

Invoke explicitly with `/skill:<name>`, or let Pi discover them from context.

**Why skills matter beyond prompt files:** each skill encodes a complete research pattern — not just instructions, but the sequence of Octocode calls that produce trustworthy answers. `octocode-research` tells the agent to orient with a tree first, then search, then read exact slices. `octocode-awareness` tells it to claim files before editing, record decisions before compacting, verify before concluding. These aren't tips. They're tested workflows the agent can activate without reinventing them every session.

---

## The memory layer: awareness

`octocode-awareness` deserves its own mention because it solves the hardest problem in multi-session or multi-agent work: **what did we decide, and who is editing what right now?**

When the awareness bridge is active (bundled `awareness.py` script present), every Pi `write` or `edit` tool call goes through a preflight check:

1. The agent claims the file with a rationale and TTL.
2. If another agent already holds the lock, the edit is blocked with a clear reason.
3. When the tool call completes, the lock is released.

This means two parallel Pi agents on the same codebase can't silently stomp each other's edits. The conflict surfaces at claim time, not at git merge time.

Before compaction, the agent can record decisions, open questions, and file states to the awareness store. The next session — or the next agent — can read them back. Context doesn't die at the context window boundary.

---

## The extension glue

The extension (`dist/index.js`) is the piece that wires all of this into Pi's lifecycle:

- **`resources_discover`** — tells Pi where the bundled skills are, so they load into every session automatically.
- **`before_agent_start`** — appends the system prompt and injects the exact bundled CLI path into the agent's context on every turn.
- **`tool_call` / `tool_result`** — hooks into Pi's tool lifecycle to run the awareness preflight and release locks.
- **Slash commands** — `/octocode-setup`, `/octocode-status`, `/octocode-mcp-install`, `/octocode-skills-update` for setup and maintenance.

None of this requires MCP. Pi core is lean by design; the extension respects that. MCP is additive, not required.

---

## Install

```bash
# Global (recommended — available in all projects)
pi install npm:@octocodeai/pi-extension

# Project-local only
pi install -l npm:@octocodeai/pi-extension

# From source (development)
yarn workspace @octocodeai/pi-extension build
pi install /path/to/octocode-mcp/packages/octocode-pi-extension
```

> Pi packages run with full system access. Review this package before installing it in a sensitive environment.

---

## Get started

Once installed, the operating model, skills, and bundled CLI are live. Verify everything is wired:

```bash
/octocode-status
```

The output shows: bundled CLI version and path, skill list, awareness bridge state, system prompt location.

To write a persistent `APPEND_SYSTEM.md` to disk (pinned across sessions and visible in your project):

```bash
# Project-local (.pi/APPEND_SYSTEM.md)
/octocode-setup

# Global (~/.pi/agent/APPEND_SYSTEM.md)
/octocode-setup --global
```

Authenticate Octocode for GitHub and npm access — the bundled path is shown by `/octocode-status`:

```bash
node <bundled-path> auth status --json
# or if octocode is also on PATH:
octocode auth status --json
```

---

## Slash commands

| Command | Purpose |
|---|---|
| `/octocode-status` | Show bundled CLI, prompt, and skills — verify everything loaded |
| `/octocode-setup` | Write managed prompt block to `.pi/APPEND_SYSTEM.md` |
| `/octocode-setup --global` | Write managed prompt block to `~/.pi/agent/APPEND_SYSTEM.md` |
| `/octocode-mcp-install [args]` | Confirm, then run the bundled `octocode install ...` for MCP clients |
| `/octocode-skills-update` | Confirm, then update the package and reload Pi resources |

---

## What ships in the package

| Asset | Source | Shipped location |
|---|---|---|
| System prompt | `docs/PI/APPEND_SYSTEM.md` | `dist/system/APPEND_SYSTEM.md` |
| Skills | root `skills/` (copied during build) | `skills/` and `dist/skills/` |
| Extension | `src/index.js` | `dist/index.js` |
| **Octocode CLI** | `octocode` npm dep (`out/`) | `dist/bin/octocode.js` + `dist/bin/chunks/` |

The CLI is physically copied from the `octocode` package into `dist/bin/` at build time. When Pi installs the package, the CLI is already on disk — no separate download, no `npx` invocation, always in sync with the extension version.

The build excludes secret env files and Python caches; `.env.example` files are kept intact.

---

## Optional: Octocode MCP inside Pi

The bundled CLI covers the full research surface. Add MCP only when you specifically want Pi to call Octocode through structured tool calls rather than shell invocations.

Install a Pi MCP adapter:

```bash
pi install npm:pi-mcp-adapter
```

Add Octocode to `.mcp.json` in your project:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"]
    }
  }
}
```

Use `~/.config/mcp/mcp.json` to make it available across all projects.

For Cursor, Claude Code, VS Code, and other MCP-native hosts:

```bash
/octocode-mcp-install --ide cursor
# or directly (using the bundled CLI):
node <bundled-path> install --ide cursor
```

---

## Development

```bash
# Build
yarn workspace @octocodeai/pi-extension build

# Build + lint + test
yarn workspace @octocodeai/pi-extension verify
```

The build: refreshes the local `skills/` mirror from root `skills/`, copies everything into `dist/`, then copies the octocode CLI from `node_modules/octocode/out/` into `dist/bin/`. An assertion fails the build if `dist/bin/octocode.js` is missing — the bundled CLI is not optional.

---

## References

- [Octocode APPEND_SYSTEM.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/PI/APPEND_SYSTEM.md)
- [Octocode Skills](https://github.com/bgauryy/octocode/blob/main/skills/README.md)
- [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md)
- [Pi extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)
- [Pi skills](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md)
- [Pi MCP adapter](https://github.com/nicobailon/pi-mcp-adapter)
- [Octocode MCP](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)
