# Octocode News

Scan whats-new across AI, developer tools, web platform, security, and notable repositories — releases, changelogs, and trend signals.

Use when the user wants latest updates, tech/AI news, or a short research brief on recent changes.

## Capabilities

- Fan out across news/release/changelog surfaces
- Rank notable repos and product updates
- Produce a concise brief with sources
- Avoid duplicating hero items inside section lists

## How It Works

```text
SCOPE TOPICS → SEARCH → DEDUPE → RANK → BRIEF
```

Prefer primary sources (release notes, changelogs, official blogs) over secondary summaries. Mark weak leads. Keep the output short enough to act on.

## Installation

```bash
npx octocode skill --name octocode-news
```

## Maintainer Notes

Keep this README user-facing. Ranking heuristics and output shape live in `SKILL.md`. After structural edits, run skill-review when `octocode-skills` is available.
