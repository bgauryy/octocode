# Octocode Documentation Skill

Evidence-backed documentation skill for humans and coding agents, with the Google developer documentation style guide built in.

## Features

- Modes: agent-docs (`AGENTS.md`), human-docs (Diátaxis), ADRs, codebase-pack (multi-file set), style-pass (copyedit or style review)
- Complete Google style guide coverage: 23 `references/style-*.md` files own every topic in the guide, mapped page by page in `references/style-sources.md`, with a drift check against the guide's own changelog
- Every reference links the guide pages it restates, `assets/google-style-pages.tsv` maps all 69 pages to their owner, and `references/style-sources.md` says when to open the live page, how to cite it, and what to do when it disagrees with the snapshot
- Full 597-entry word list as data in `assets/google-word-list.tsv` (term, verdict, guidance), rebuildable from the live guide
- `scripts/style-lint.mjs` — 36 deterministic Markdown checks in three levels (ERROR gates, WARN mechanical, INFO judgment): sentence case, heading hierarchy, vague link text, missing alt text, non-inclusive terms, time-anchored wording, placeholders, passive voice, serial comma, plus word-list terms
- `scripts/refresh-word-list.mjs` — rebuild the word-list data from the live guide (`--dry-run` reports the diff)
- `scripts/style-lint.mjs --self-test` — built-in good/bad fixtures prove every rule still fires, including that no ERROR gate has gone inert
- Suppression is per file (`<!-- style-lint: ignore-file -->`) or per line (`<!-- style-lint: ignore-line rule-id -->`), for the pages that must quote a term the rules ban
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
| Which guide page backs a rule? | `references/style-sources.md`, or the Upstream pages section of the owning reference |
| Is the rule still what the guide says? | Open the linked page; `references/style-sources.md` has the verify procedure |
| Is this word allowed? | `grep -iP "^term\t" assets/google-word-list.tsv` |
| What breaks the build? | `node scripts/style-lint.mjs docs/` — exit 1 on ERROR |
| Is the word list current? | `node scripts/refresh-word-list.mjs --dry-run` |
| Has the guide changed since? | [What's new](https://developers.google.com/style/whats-new), then `references/style-sources.md` |

## Resources

Every rule in `references/style-*.md` restates a page of Google's guide and links it from the reference's Upstream line; `assets/google-style-pages.tsv` maps all 69 pages to their owning reference; `references/style-sources.md` says when and how to check the live page. Start at one of these:

| Resource | Use for |
|---|---|
| [Google developer documentation style guide](https://developers.google.com/style) | The upstream guide these references restate |
| [Highlights](https://developers.google.com/style/highlights) | The short list of rules that cover most reviews |
| [What's new](https://developers.google.com/style/whats-new) | The guide's changelog — the drift check for every reference here |
| [Word list](https://developers.google.com/style/word-list) | The upstream source of `assets/google-word-list.tsv` |
| [Diátaxis](https://diataxis.fr/) | The doc-type framework behind `references/diataxis.md` |
| [agents.md](https://agents.md/) | The spec behind `references/agents-md.md` |

## Audiences

| Audience | Use for |
|----------|---------|
| Users / maintainers | README, API docs, runbooks, ADRs, AGENTS.md index, style reviews |
| Developers extending the skill | refs under `references/`, review with `octocode-skills` |
| Coding agents | activation through description triggers; follow lobby routes |

## Installation

```bash
npx octocode skill install octocode-documentation
```

Install from a checkout of this repository with `npx octocode skill install octocode-documentation --path <dir>`, or copy or symlink the folder into `.cursor/skills/octocode-documentation` or `.agents/skills/octocode-documentation`.

This repository vendors the skill at `skills/octocode-documentation`. Google publishes its style guide content under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); this skill restates the rules and keeps the word-list guidance strings as data, sourced page by page in `references/style-sources.md`.
