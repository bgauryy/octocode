# Octocode skills

Agent Skills are reusable workflows that teach an agent *how* to do a kind of work with Octocode's tools: research a claim, review an architecture, write docs, or measure a change. Each skill is a folder: `SKILL.md` defines the agent behavior and triggers, and `README.md` gives the human overview. The folders in this directory are what `octocode skill install` publishes.

Tested skills that are not yet published live in [`../skills-beta/`](../skills-beta/). Skills for developing this repository live in [`../skills-dev/`](../skills-dev/).

## Choose a skill

Pick by the job in front of you. Each row says when the skill applies and when another one fits better.

### Research and understanding

| Skill | Use it when | Use something else when |
|---|---|---|
| [octocode-research](octocode-research/) | A code claim needs evidence before you assert it: trace callers, imports, runtime wiring, a regression, GitHub history, or change impact. Also for "research this" or "use octocode" | The fix is already known → do it directly. Open-ended ideas → octocode-brainstorming |
| [octocode-clasify](octocode-clasify/) | You explicitly want a typed judgment, or need to locate a semantic answer inside a known but unread file before reading it | The target is a literal or an identifier → use direct search |
| [octocode-scraping](octocode-scraping/) | You want public web pages or a whole site saved as a local, cited corpus for repeated queries: docs, pricing tables, link maps | The page is JS-rendered or needs interaction → octocode-chrome-devtools |
| [octocode-chrome-devtools](octocode-chrome-devtools/) | You need a real running browser: JS-rendered pages, live DOM, clicks, network (HAR) capture, console, or performance traces, signed-in sessions | The page is static and public → octocode-scraping |

### Design and decisions

| Skill | Use it when | Use something else when |
|---|---|---|
| [octocode-brainstorming](octocode-brainstorming/) | An idea is still open and needs options, feasibility checks, or scope exploration before building | The decision is settled → implement it |
| [octocode-architect](octocode-architect/) | An architecture decision or refactor needs evidence about boundaries, contracts, data and control flow, coupling, blast radius, cycles, or performance | You only need facts → octocode-research. Behavior-preserving cleanup → octocode-clean-agentic-code |
| [octocode-rfc-generator](octocode-rfc-generator/) | A consequential change (architecture, migration, public contract, multi-phase work) needs a written, reviewed decision | The edit is trivial, or you're still ideating |
| [octocode-exploratory-thinking](octocode-exploratory-thinking/) | You explicitly want an exploratory, out-of-the-box pass on a problem | Ordinary analysis |

### Code quality

| Skill | Use it when | Use something else when |
|---|---|---|
| [octocode-roast](octocode-roast/) | You want a blunt, evidence-backed critique that ranks smells, debt, hot paths, and cleanup priorities | You want the cleanup done → octocode-clean-agentic-code |
| [octocode-clean-agentic-code](octocode-clean-agentic-code/) | Behavior-preserving cleanup: dead exports, shims, duplicate logic, stale config, tests, or docs, god files, agent residue | Feature work or behavior changes |

### Docs, prompts, and skills

| Skill | Use it when | Use something else when |
|---|---|---|
| [octocode-documentation](octocode-documentation/) | Creating, repairing, or reviewing READMEs, API docs, guides, comments, ADRs, runbooks, or stale docs | — |
| [octocode-prompt-optimizer](octocode-prompt-optimizer/) | A prompt, agent contract, MCP instruction, tool or schema description, policy, or handoff must change agent behavior | Skill structure or triggers → octocode-skills |
| [octocode-skills](octocode-skills/) | Finding, comparing, reviewing, creating, repairing, installing, syncing, or tuning the triggers of Agent Skills | — |

### Measurement and multi-agent work

| Skill | Use it when | Use something else when |
|---|---|---|
| [octocode-eval-benchmark](octocode-eval-benchmark/) | Proving a change helped: evals, baselines, held-out cases, judge calibration, keep/discard loops | Passing tests are enough |
| [octocode-subagent](octocode-subagent/) | Work has independent lanes worth delegating: parallel workers, local Ollama offload, or agent-to-agent handoffs | Routine edits, or dependent steps one call can handle |
| [octocode-agents-communication](octocode-agents-communication/) | Several agents or sessions share work: discover peers, reserve files before editing, exchange results, and handoffs | Solo work with no collaboration signal |

### Beta (tested, not published)

| Skill | Use it when |
|---|---|
| [octocode-architecture-view](../skills-beta/octocode-architecture-view/) | You want a system's architecture as an interactive HTML map: layers, modules, dependencies, runtime flows, data stores |

### For developing this repository

| Skill | Use it when |
|---|---|
| [octocode-dev](../skills-dev/octocode-dev/) | Any development work in this repository: build/test/verify through its task runner, contract/native/config changes, release, and end-to-end tool audits |
| [octocode-context-audit](../skills-dev/octocode-context-audit/) | Agent sessions feel bloated or slow, or you are about to add instructions, skills, or MCP servers. It measures what loads every session and what gets used |
| [rust-best-practices](../skills-dev/rust-best-practices/) | Writing or reviewing Rust in this repository and a design choice is open: crates, errors, workspace layout, performance, unsafe code |

## Install

```bash
npx -y octocode skill list                          # bundled skills and install status
npx -y octocode skill install octocode-research     # install one
```

`--platform` links the skill into a host: `pi`, `claude`, `cursor`, `codex`, `opencode`, `copilot`, `gemini`, or `all`. Installed copies live under `~/.octocode/skills`, and this `skills/` directory stays the source of truth. `octocode skill check` verifies installs and links. `octocode skill remove <name>` drops a skill left over from an older release.

## Folder contract

- `SKILL.md` owns triggers, workflow, gates, and routes.
- `README.md` explains when and why people use the skill.
- `references/`, `scripts/`, and `assets/` contain routed depth or runtime resources.
- Keep every local file reference inside its skill folder. Use every shipped file.
- Keep chat-only results in chat. Store new artifacts in the skill's workspace `.octocode/` path; keep approved source or configuration edits at their named targets.

## Verify

```bash
node skills/octocode-skills/scripts/skill-review.mjs skills
```

The review must finish with zero errors. Resolve warnings or document why they are intentional.
