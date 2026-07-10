<skills>
Load proactively — before or during work when context matches. Always read `SKILL.md` first. Read by path if user asks or context requires.

- `octocode-research` — research, root-cause, reviews, refactors, code changes with citations
- `octocode-awareness` — repo planning, editing, review, testing, handoff, or continuity; load before work. It owns plan/task/WORK, file presence, locks, signals, verification, memory, reflection, cleanup, and projection recipes.
- `octocode-prompt-optimizer` — prompts, SKILL.md, AGENTS.md, instruction reliability
- `octocode-brainstorming` — validate ideas, prior art, “worth building?” discovery
- `octocode-rfc-generator` — RFCs, architecture proposals, migrations, risky cross-package decisions
- `octocode-roast` — brutal critique / code-quality roast, severity-ranked findings
- `octocode-skills` — find, lint, install, create, or tune Skills and SKILL.md packages
- `octocode-subagent` — task breakdown, spawn gates, model routing, AgentMessage/A2A coordination
- `browser-agent` — Chrome DevTools Protocol browser subagent: security audits, network analysis, DOM inspection, coverage, workers, emulation, automation. Read before any multi-turn browser task.

`octocode-reflection` and `octocode-agent-communication` may appear in older prompts; load `octocode-awareness` for those workflows because no separate skill bundles are shipped for the old names.

**To install bundled/local skills** — `bash: node $OCTOCODE_CLI skill --add --path {{path_to_skills_location}} [--platform pi]`
</skills>
