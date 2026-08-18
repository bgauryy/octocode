# Octocode Documentation Skill

Evidence-backed documentation skill for humans and coding agents, with the Google developer documentation style guide built in.

## Features

- Modes: agent-docs (`AGENTS.md`), human-docs (Diátaxis), ADRs, codebase-pack handoff, style-pass (copyedit or style review)
- Complete Google style guide coverage: 23 `references/style-*.md` files own every topic in the guide, mapped page by page in `references/style-sources.md`, with a drift check against the guide's own changelog
- Full 597-entry word list as data in `assets/google-word-list.tsv` (term, verdict, guidance), rebuildable from the live guide
- `scripts/style-lint.mjs` — 36 deterministic Markdown checks in three levels (ERROR gates, WARN mechanical, INFO judgment): sentence case, heading hierarchy, vague link text, missing alt text, non-inclusive terms, time-anchored wording, placeholders, passive voice, serial comma, plus word-list terms
- `scripts/refresh-word-list.mjs` — rebuild the word-list data from the live guide (`--dry-run` reports the diff)
- `scripts/style-lint.mjs --self-test` — built-in good/bad fixtures prove every rule still fires, including that no ERROR gate has gone inert
- Durable cross-refs — no brittle line citations or code dumps by default
- Outline gate before writes; style gate before done

## How it works

1. Choose mode (`references/modes.md`)
2. Research with durable evidence (`references/evidence-research.md`)
3. Classify and draft using mode refs + `references/agent-readable.md`
4. Gate outline, write, verify (`references/write-verify.md`)
5. Style pass: `node scripts/style-lint.mjs <paths>`, then fix each hit with the reference named in the message (`references/style-index.md`)

The linter covers Markdown; docstrings, HTML, and UI strings are hand-checked against the same references.

## Style lookups

| Question | Answer |
|----------|--------|
| Which reference owns a topic? | `references/style-index.md` |
| Which guide page backs a rule? | `references/style-sources.md` |
| Is this word allowed? | `grep -iP "^term\t" assets/google-word-list.tsv` |
| What breaks the build? | `node scripts/style-lint.mjs docs/` — exit 1 on ERROR |
| Is the word list current? | `node scripts/refresh-word-list.mjs --dry-run` |
| Has the guide changed since? | `https://developers.google.com/style/whats-new`, then `references/style-sources.md` |

## Audiences

| Audience | Use for |
|----------|---------|
| Users / maintainers | README, API docs, runbooks, ADRs, AGENTS.md index, style reviews |
| Developers extending the skill | refs under `references/`, review with `octocode-skills` |
| Coding agents | activation through description triggers; follow lobby routes |

## Installation

```bash
npx octocode skill --add --path skills/octocode-documentation
```

Alternative: copy or symlink into `.cursor/skills/octocode-documentation` or `.agents/skills/octocode-documentation`.

This repository vendors the skill at `skills/octocode-documentation`. Google's style guide content is published under CC BY 4.0; rules are restated here, and word-list guidance strings are kept as data, sourced page by page in `references/style-sources.md`.
