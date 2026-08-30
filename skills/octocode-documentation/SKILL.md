---
name: octocode-documentation
description: "Use when docs are missing, wrong, stale, or badly written, or need a copyedit against the Google style guide: README, API reference, runbook, CONTRIBUTING, changelog, onboarding, AGENTS.md/CLAUDE.md, ADR, migration guide, Diátaxis or whole-codebase restructure, docstrings, alt text, prose linting. Not for code, commits, or marketing copy. Code investigation → octocode-research; SKILL.md folders → octocode-skills."
---

# Octocode Documentation

Evidence-backed docs for humans and agents, written to the Google developer documentation style guide. Classify first. Gate writes. Prefer durable cross-refs over code dumps.

## Flow

`UNDERSTAND → RESEARCH → CLASSIFY → OUTLINE GATE → WRITE → STYLE → VERIFY`

Workspace output contract: chat-only reviews stay in chat. New generated reports or unnamed drafts default to `<workspace>/.octocode/octocode-documentation/`; scratch data uses `<workspace>/.octocode/tmp/octocode-documentation/`. Requested documentation and approved source edits keep their named repository paths. Never fall back to a user-level Octocode home for artifacts.

UNDERSTAND names the deliverable, audience, and target paths. Compress when the request already names targets and type. Expand when claims need verification. A copyedit request starts at STYLE. Answer a single-term question ("is `allows you to` okay?") straight from `assets/google-word-list.tsv` — quote the guidance and stop.

## Rules

- Verify claims in the repository before asserting them. Invented commands, paths, APIs, and env vars are the one unrecoverable failure — omit or mark "Not verified in repository" instead.
- Pick one mode and load its routes before writing.
- Gate creates and overwrites unless the requester approved the targets this turn; a copyedit of a named file carries its own approval. Touch only the files they named — propose the rest.
- Apply the style defaults in `references/style-index.md` to every line you write or edit, and name the rule when you change someone else's wording.
- The style references are a snapshot of Google's guide, and every one links the pages it restates. IF someone disputes a rule, the pack doesn't carry it, or the wording carries risk (trademark, product name, legal claim, security claim) → THEN open the live page with a web tool, quote it with its URL, and fix the reference when it disagrees; say so when no fetch was possible.
- A style pass changes wording, not claims; a fact change goes back to RESEARCH.
- `AGENTS.md` is an index of links and non-obvious rules, not a content dump.
- Prefer durable pointers (module path, contract name, doc link) over line numbers and pasted code.
- One Diátaxis type per page; link siblings instead of mixing.
- IF the project documents its own style guide, or the repository already applies a convention consistently → THEN follow it and report the conflict instead of adding a second scheme.

Stop when: outline gate awaits answer; write+style+verify finishes; a word-list lookup answered the question; a missing fact makes the doc dishonest to write (otherwise mark "Not verified in repository" and continue); conventions conflict; user cancels.

## Workflows — the mode fixes the route order

| Mode | Deliverable | Route order |
|---|---|---|
| agent-docs | `AGENTS.md`, nested agent instructions, `CLAUDE.md` symlink | `modes.md` → `evidence-research.md` → `agents-md.md` → `agent-readable.md` → `write-verify.md` → `style-lint.mjs` |
| human-docs | README, tutorial, how-to, reference, explanation, runbook | `modes.md` → `evidence-research.md` → `diataxis.md` → `agent-readable.md` → `write-verify.md` → `style-lint.mjs` |
| adr | Architecture decision record | `modes.md` → `evidence-research.md` → `adr.md` → `write-verify.md` → `style-lint.mjs` |
| codebase-pack | Multi-file docs set | `modes.md` → plan the file set → gate once → per file: `diataxis.md` → `write-verify.md` → `style-lint.mjs` |
| style-pass | Edited text, or a style review report | `style-index.md` → the owning `style-*.md` → `style-lint.mjs` → `style-review.md` for a report someone else acts on |

## Routes — workflow references

| Phase | When | Read |
|---|---|---|
| CLASSIFY | Choosing mode or audience | `references/modes.md` |
| RESEARCH | Gathering or verifying repository facts | `references/evidence-research.md` |
| CLASSIFY | Choosing the Diátaxis type for human-docs, or reviewing one | `references/diataxis.md` |
| WRITE | Writing or updating agent instruction files | `references/agents-md.md` |
| WRITE | Recording a decision | `references/adr.md` |
| WRITE | Cross-refs, density, durability — read before the first line | `references/agent-readable.md` |
| OUTLINE GATE, WRITE, VERIFY | Outline gate, write steps, style pass, verify checklist | `references/write-verify.md` |
| STYLE | Any wording, formatting, or terminology question — maps every guide topic to its owner | `references/style-index.md` |

## Routes — style pack, grouped; `references/style-index.md` owns the per-topic map

| Ask | Read |
|---|---|
| Which reference owns this topic — every guide topic, one row each | `references/style-index.md` |
| Prose: tone, person, voice, tense, grammar, one specific word, abbreviations, jargon, translation, inclusive terms | `references/style-voice.md`, `references/style-grammar.md`, `references/style-words.md`, `references/style-abbreviations.md`, `references/style-global.md`, `references/style-inclusive.md` |
| Page shape: headings, lists, numbered steps, notices, tables, footnotes, figures, alt text | `references/style-structure.md`, `references/style-procedures.md`, `references/style-blocks.md`, `references/style-images.md` |
| Mechanics: bold/italic/code choice, capitalization, filenames, markup, punctuation, numbers, dates, units | `references/style-format.md`, `references/style-punctuation.md`, `references/style-numbers.md` |
| Technical text: code font, samples, command syntax, placeholders, example values, UI wording, link text | `references/style-code.md`, `references/style-cli.md`, `references/style-examples.md`, `references/style-ui.md`, `references/style-links.md` |
| Claims and reference text: time words, superlatives, product names, trademarks, third-party text, docstrings | `references/style-claims.md`, `references/style-api.md` |
| Producing a review someone else acts on, or checking a rule against the live guide | `references/style-review.md`, `references/style-sources.md` |

## Scripts and assets — what to run, when, and how

| Run | When | How it behaves |
|---|---|---|
| `scripts/style-lint.mjs <paths>` | At STYLE, before hand-reading prose | Markdown only. ERROR gates, WARN is mechanical, INFO needs judgment. One finding per rule per line; `--max-per-rule` caps per file (default 20); `--only`/`--skip` select rules; `--json` for machine output. Exit 1 on ERROR, or on WARN with `--strict`; 2 on bad usage |
| `scripts/style-lint.mjs --self-test` | After editing any rule | Lints built-in good/bad fixtures so a gate cannot go inert |
| `scripts/refresh-word-list.mjs --dry-run` | When the word list looks stale | Rebuilds `assets/google-word-list.tsv` from `developers.google.com`; it fetches even with `--dry-run`, and refuses to write a short parse |
| `assets/google-word-list.tsv` | Answering a single-term question | 597 guide entries with verdict and guidance; quote it and stop |
| `assets/google-style-pages.tsv` | Finding which reference owns a guide page, or its URL | All 69 pages as `slug`, `title`, `owner`, `url`; `grep -P "^tables\t"` answers ownership, and the URL is what you fetch to verify |

Suppression: `<!-- style-lint: ignore-file -->` skips a file found by recursion, `<!-- style-lint: ignore-line rule-id -->` mutes named rules on one line. Both are inert inside a code span, so a page can document them. Docstrings, HTML, and UI strings stay hand-checked.

## Related

- Pure code or repository evidence with no docs deliverable → `octocode-research`; authoring a `SKILL.md` → `octocode-skills`.
- Full multi-file pack → plan the file set, gate it once, then work file by file.
- Unclear mode → ask once: agent-docs / human-docs / adr / codebase-pack / style-pass. No Octocode → host search tools.
- Measuring whether this skill triggers and holds its routes → `octocode-eval-benchmark`; the runnable sensors here are `scripts/style-lint.mjs --self-test` (rules still fire) and `scripts/refresh-word-list.mjs --dry-run` (word-list drift).
