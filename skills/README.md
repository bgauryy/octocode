# Octocode skills

Public Agent Skills. Each skill is a standalone folder whose `SKILL.md` defines agent behavior and whose `README.md` provides the human overview. These folders are what `octocode skill install` publishes.

Tested skills that are not ready to publish live in [`../skills-beta/`](../skills-beta/). Skills for working on this repository live in [`../skills-dev/`](../skills-dev/).

## Choose a skill

| Need | Skill |
|---|---|
| Discover peers, send messages, and reserve shared paths | [octocode-agents-communication](octocode-agents-communication/) |
| Investigate code, packages, history, or a failure | [octocode-research](octocode-research/) |
| Review or refactor architecture, algorithms, dependencies, flows, interfaces, or maintainability | [octocode-architect](octocode-architect/) |
| See a system's layers, modules, flows, stores, and dependencies as an interactive HTML map | [octocode-architecture-view](octocode-architecture-view/) |
| Explore whether an idea is worth building | [octocode-brainstorming](octocode-brainstorming/) |
| Think through a task with an exploratory awareness shift, or with a named substance as a presence | [octocode-exploratory-thinking](octocode-exploratory-thinking/) |
| Make a consequential design or migration decision, with optional Jev review | [octocode-rfc-generator](octocode-rfc-generator/) |
| Measure whether a change improved behavior | [octocode-eval-benchmark](octocode-eval-benchmark/) |
| Orchestrate workers or offload sealed work to local Ollama | [octocode-subagent](octocode-subagent/) |
| Write, restructure, or copyedit documentation | [octocode-documentation](octocode-documentation/) |
| Deliver a blunt, evidence-backed code critique | [octocode-roast](octocode-roast/) |
| Remove dead code, shims, and agent residue without changing behavior | [octocode-clean-agentic-code](octocode-clean-agentic-code/) |
| Improve prompts, policies, handoffs, or tool schemas | [octocode-prompt-optimizer](octocode-prompt-optimizer/) |
| Discover, create, review, install, or synchronize skills | [octocode-skills](octocode-skills/) |
| Debug a live page with Chrome DevTools evidence | [octocode-chrome-devtools](octocode-chrome-devtools/) |
| Turn public pages into a local cited corpus | [octocode-scraping](octocode-scraping/) |
| Explicit classification requests or experiments; the linked skill owns the benefit gate | [octocode-clasify](octocode-clasify/) |

## Install

```bash
npx -y octocode skill list
npx -y octocode skill install octocode-research
```

Use `--platform pi,claude,cursor,codex` to select one or more supported hosts. The source of truth remains this `skills/` directory.

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
