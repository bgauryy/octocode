<skills>
Load proactively — before or during work when context matches. Always read `SKILL.md` first. Read by path if user asks or context requires.

- `octocode-research` — research, root-cause, reviews, refactors, code changes with citations
- `octocode-awareness` — attend first; inspect Ready/Claimed/Verify/FilesUnderWork, claim a matching shared-plan task or open standalone WORK, declare advisory file presence for every edit, use exclusivity only for sensitive paths, then verify, hand off, and clean stale state
- `octocode-prompt-optimizer` — prompts, SKILL.md, AGENTS.md, instruction reliability
- `octocode-brainstorming` — validate ideas, prior art, “worth building?” discovery
- `octocode-rfc-generator` — RFCs, architecture proposals, migrations, risky cross-package decisions
- `octocode-roast` — brutal critique / code-quality roast, severity-ranked findings
- `octocode-skills` — find, lint, install, create, or tune Skills and SKILL.md packages
- `octocode-subagents` — spawn, coordinate, and synthesize parallel subagent workers
- `browser-agent` — Chrome DevTools Protocol browser subagent: security audits, network analysis, DOM inspection, coverage, workers, emulation, automation. Read before any multi-turn browser task.

`octocode-reflection` and `octocode-agent-communication` may appear in older prompts; load `octocode-awareness` for those workflows because no separate skill bundles are shipped for the old names.

**To install bundled/local skills** — `bash: node $OCTOCODE_CLI skill --add --path {{path_to_skills_location}} [--platform pi]`
</skills>
