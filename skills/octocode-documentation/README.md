# Octocode Documentation Skill

Evidence-backed documentation skill for humans and coding agents, with the Google developer documentation style guide built in.

## What it covers

- Five modes: agent instructions, human docs, ADRs, multi-file packs, and style passes.
- Topic-routed Google style references, backed by a 597-entry word list and a 69-page source map.
- A deterministic Markdown linter with ERROR, WARN, and INFO findings plus a self-test.
- Evidence and approval gates for repository claims, creates, and overwrites.
- Durable cross-references instead of copied code or line-number links.

## How it works

1. Choose a mode in `references/modes.md`.
2. Verify claims with `references/evidence-research.md`.
3. Follow the mode route in `SKILL.md` and the gate in `references/write-verify.md`.
4. Run `node scripts/style-lint.mjs <paths>`, then interpret findings through `references/style-index.md`.

The linter covers Markdown; docstrings, HTML, and UI strings are hand-checked against the same references.

## Style lookups

| Question | Answer |
|----------|--------|
| Which reference owns a topic? | `references/style-index.md` |
| Which live page backs a rule? | `references/style-sources.md` |
| Is this word recommended? | Search `assets/google-word-list.tsv` |
| What breaks the build? | `node scripts/style-lint.mjs docs/` — exit 1 on ERROR |
| Is the word list current? | `node scripts/refresh-word-list.mjs --dry-run` |
| Has the guide changed since? | [What's new](https://developers.google.com/style/whats-new), then `references/style-sources.md` |

## Sources

| Resource | Use for |
|---|---|
| [Google developer documentation style guide](https://developers.google.com/style) | The upstream guide these references restate |
| [Word list](https://developers.google.com/style/word-list) | The upstream source of `assets/google-word-list.tsv` |
| [Diátaxis](https://diataxis.fr/) | The doc-type framework behind `references/diataxis.md` |
| [agents.md](https://agents.md/) | The spec behind `references/agents-md.md` |

## Installation

```bash
npx octocode skill install octocode-documentation
```

For a local checkout, add `--path <dir>`. The bundled copy lives at `skills/octocode-documentation`. Google publishes its guide under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); `references/style-sources.md` records source ownership.
