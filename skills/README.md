# Octocode skills

Canonical Agent Skills for this monorepo. Each skill is a standalone folder whose `SKILL.md` defines agent behavior and whose `README.md` provides the human overview.

## Choose a skill

| Need | Skill |
|---|---|
| Investigate code, packages, history, or a failure | [octocode-research](octocode-research/) |
| Analyze dependency topology, cycles, impact, or dead-code candidates | [octocode-code-graph](octocode-code-graph/) |
| Explore whether an idea is worth building | [octocode-brainstorming](octocode-brainstorming/) |
| Make a consequential design or migration decision | [octocode-rfc-generator](octocode-rfc-generator/) |
| Measure whether a change improved behavior | [octocode-eval-benchmark](octocode-eval-benchmark/) |
| Orchestrate workers or offload sealed work to local Ollama | [octocode-subagent](octocode-subagent/) |
| Write, restructure, or copyedit documentation | [octocode-documentation](octocode-documentation/) |
| Deliver a blunt, evidence-backed code critique | [octocode-roast](octocode-roast/) |
| Improve prompts, policies, handoffs, or tool schemas | [octocode-prompt-optimizer](octocode-prompt-optimizer/) |
| Discover, create, review, install, or synchronize skills | [octocode-skills](octocode-skills/) |
| Debug a live page with Chrome DevTools evidence | [octocode-chrome-devtools](octocode-chrome-devtools/) |
| Turn public pages into a local cited corpus | [octocode-scraping](octocode-scraping/) |
| Apply architect-level rigor to consequential code work | [octocode-architect](octocode-architect/) |

## Install

```bash
npx octocode skill list
npx octocode skill install octocode-research --platform codex
```

Use `--platform pi,claude,cursor,codex` to select one or more supported hosts. The source of truth remains this `skills/` directory.

## Folder contract

- `SKILL.md` owns triggers, workflow, gates, and routes.
- `README.md` explains when and why a human would use the skill.
- `references/`, `scripts/`, and `assets/` contain routed depth or runtime resources.
- Every local file reference stays inside its skill folder, and every shipped file must be used.
- Chat-only results stay in chat. New artifacts use the skill's workspace `.octocode/` path; approved source or configuration edits keep their named targets.

## Verify

```bash
node skills/octocode-skills/scripts/skill-review.mjs skills
```

The review must finish with zero errors. Resolve warnings or document why they are intentional.
