# @octocodeai/pi-extension

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-pi-extension/assets/logo.png" width="640px" alt="Octocode + Pi">
</div>

The Octocode harness for [Pi](https://github.com/earendil-works/pi). One install gives the agent a research engine, a persistent memory, an engineering operating model, and subagent coordination — active on every turn.

```bash
pi install npm:@octocodeai/pi-extension
```

---

## What the agent gets

**A research engine.** The agent uses [Octocode](https://octocode.ai) for all code discovery — local files, GitHub, npm, LSP semantics, AST patterns, binaries. One tool instead of grep, gh, and curl. It reads lean: locate first, then understand structure, then read exactly.

**Live web access.** A single `web` tool lets the agent search the web and read pages as clean text — for docs, changelogs, error messages, and anything beyond the codebase or its training cutoff. Works with **zero config** (DuckDuckGo); add a Tavily or Serper key for higher-quality, AI-answered search. Fetches are SSRF-guarded and browser-realistic.

**An engineering operating model.** A system prompt shapes how the agent reasons on every turn: research before writing, prove before claiming done, verify after every change. It reads git state and project config before acting. It maps blast radius before touching anything. It builds incrementally — reuse before adding, minimum that works, one owner per behavior, fix root causes not call sites, leave no traps.

**Clean code and architecture.** The agent writes code with intent-named functions, single responsibility, guard-clause returns, and no speculative params. It respects Clean Architecture — core logic free of I/O and framework, side effects at the edges, dependencies pointing inward. Types and schemas are treated as contracts: producers and consumers change together, every type change fixes every error.

**Persistent memory.** Before starting work the agent queries a shared memory store for prior lessons, decisions, and gotchas. After finishing it records what it learned — findings, failures, decisions — so the next session starts knowing what this one discovered. The same store is shared across all Octocode tools and agents on the machine.

**Self-improvement.** The agent tracks recurring failures by signature and surfaces patterns. It proposes improvements to its own behavior after tasks. A human reviews and merges them.

**Subagent coordination.** The agent forks work to independent sub-sessions when tasks are self-contained and unrelated to the current context. It writes a plan before complex work, compacts to that plan, then executes step by step. Parallel tasks that touch the same files are isolated and merged sequentially. Context is managed autonomously — compact, clear, or hand off — without asking.

**Safe by default.** Every file edit is locked before it runs and released after, via hooks — no two agents collide. Destructive actions require confirmation. Protected files surface before editing.

---

## Research workflows

Six skills ship with the bundle:

| | |
|---|---|
| `octocode-research` | Evidence-first investigation: code exploration, implementation planning, PR review, refactoring, architecture mapping |
| `octocode-brainstorming` | Idea validation against real evidence before building — outputs a decision, not code |
| `octocode-rfc-generator` | Structured proposal for risky or cross-cutting changes, before any code is written |
| `octocode-roast` | Adversarial code review with ranked findings and specific fix paths |
| `octocode-prompt-optimizer` | Fix agent instructions that drift, skip steps, or produce inconsistent output |
| `octocode-skills` | Find, install, and author new skills |

Invoke directly with `/skill:<name>`, or the agent picks the right one from context. Skills compose — research into roast, brainstorming into RFC — for end-to-end workflows.

---

## Setup

```bash
pi install npm:@octocodeai/pi-extension
npx octocode auth login     # GitHub access for code research
/octocode-status            # confirm everything loaded (skills, memory, prompt, web provider)
```

That's it — everything below is optional.

---

## Configuration

All optional. Put keys and settings in a `.env` under your Octocode home — `~/.octocode/.env` (global) or `<project>/.octocode/.env` (per-repo, overrides global). It's loaded automatically at session start and shared with the bundled CLI, skills, and the `web` tool.

```bash
# ~/.octocode/.env   — every line optional
TAVILY_API_KEY=tvly-...      # richer web search: AI answer + results  → app.tavily.com
SERPER_API_KEY=...           # Google SERP web search                  → serper.dev
# OCTOCODE_WEB_USER_AGENT=…  # override the browser UA used for web fetch
```

- **Web search** auto-picks the best available provider: **Tavily → Serper → DuckDuckGo** (no key needed — DuckDuckGo is the fallback).
- **GitHub tokens do *not* go here** — run `npx octocode auth login` (encrypted) or use your shell env. They're protected keys and won't be read from `.env`.
- Values are never logged. Full reference: [CONFIGURATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/CONFIGURATION.md).

---

## Commands

| | |
|---|---|
| `/octocode-status` | Confirm CLI, skills, memory, and prompt all loaded |
| `/octocode-setup` | Pin the operating model to your project |
| `/octocode-setup --global` | Pin the operating model globally |
| `/octocode-skills-update` | Update to the latest version |

---

**Prefer one command?** [`octocode-agent`](../octocode-agent) bundles Pi + this harness into a single branded launcher (`octocode-agent`) with the harness leading the prompt.

---

[Octocode](https://octocode.ai) · [GitHub](https://github.com/bgauryy/octocode-mcp) · [Configuration](https://github.com/bgauryy/octocode-mcp/blob/main/docs/CONFIGURATION.md) · [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) · [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md)
