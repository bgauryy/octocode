# Octocode Documentation

Create, repair, or review documentation for humans and coding agents, with style guidance, Markdown checks, and verified claims.

Use when a README, tutorial, how-to, reference, runbook, ADR, or migration guide is missing or stale; when agent instruction files (`AGENTS.md`, `CLAUDE.md`) need restructuring; or when prose needs a fact or style review. Not for: architecture decisions (`octocode-research`, then `octocode-rfc-generator`), skill folders (`octocode-skills`), or ideation (`octocode-brainstorming`).

Workflow: `UNDERSTAND → RESEARCH → CLASSIFY → OUTLINE GATE → WRITE → STYLE → VERIFY`. The Markdown linter reports ERROR, WARN, and INFO; check non-Markdown text by hand.

```bash
npx -y octocode skill install octocode-documentation
node scripts/style-lint.mjs README.md     # maintainer check
node scripts/style-lint.mjs --self-test
```

Then run the `octocode-skills` review against this folder. Style sources: [Google developer documentation style guide](https://developers.google.com/style) · [Diátaxis](https://diataxis.fr/) · [agents.md](https://agents.md/)
