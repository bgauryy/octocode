---
name: octocode-documentation
description: "Use when docs are missing, wrong, stale, or badly written, or need a copyedit against the Google style guide: README, API reference, runbook, CONTRIBUTING, changelog, onboarding, AGENTS.md/CLAUDE.md, ADR, migration guide, Diátaxis or whole-codebase restructure, docstrings, alt text, prose linting. Not for code, commits, or marketing copy. Code investigation → octocode-research; SKILL.md folders → octocode-skills."
---

# Octocode Documentation

Write evidence-backed documentation for humans and agents. Classify the deliverable, verify repository facts, and prefer durable links over copied implementation detail.

## Flow

Flow: `UNDERSTAND → RESEARCH → CLASSIFY → OUTLINE GATE → WRITE → STYLE → VERIFY`

Chat-only reviews stay in chat. Put unnamed drafts in `<workspace>/.octocode/octocode-documentation/` and scratch data in `<workspace>/.octocode/tmp/octocode-documentation/`. Named or approved edits keep their requested repository paths. Do not fall back to a user-level Octocode home for artifacts.

UNDERSTAND identifies the deliverable, audience, approved paths, and facts that still need evidence.

## Rules

- Verify commands, paths, APIs, environment variables, and behavioral claims in the repository. Omit unsupported claims or label them "Not verified in repository".
- Choose one mode and load only its route. A named-file copyedit starts at STYLE.
- Get approval before creating or overwriting targets unless this turn already names them. Propose edits outside the approved paths.
- Apply `references/style-index.md`; when changing another writer's wording, identify the rule.
- For disputed, missing, legal, trademark, product, security, or public-API guidance, check the linked live Google page. The live guide wins; note when verification is unavailable.
- A style pass changes wording, not claims; a fact change goes back to RESEARCH.
- `AGENTS.md` is an index of links and non-obvious rules, not a content dump.
- Prefer durable pointers (module path, contract name, doc link) over line numbers and pasted code.
- One Diátaxis type per page; link siblings instead of mixing.
- Follow an established project style over this pack and report meaningful conflicts.

## Modes and routes

| Mode | Deliverable | Route order |
|---|---|---|
| agent-docs | `AGENTS.md`, nested instructions, `CLAUDE.md` entrypoint | `references/modes.md` → `references/evidence-research.md` → `references/agents-md.md` → `references/agent-readable.md` → `references/write-verify.md` |
| human-docs | README, tutorial, how-to, reference, explanation, runbook | `references/modes.md` → `references/evidence-research.md` → `references/diataxis.md` → `references/agent-readable.md` → `references/write-verify.md` |
| adr | Architecture decision record | `references/modes.md` → `references/evidence-research.md` → `references/adr.md` → `references/write-verify.md` |
| codebase-pack | Multi-file documentation set | `references/modes.md` → plan and gate the set once → per file: `references/diataxis.md` → `references/write-verify.md` |
| style-pass | Copyedit or style-review report | `references/style-index.md` → owning style reference → `style-lint.mjs`; add `references/style-review.md` for a report |

For style work, start at `references/style-index.md`, then load only the matching group:

- When editing prose or terms: `references/style-voice.md`, `references/style-grammar.md`, `references/style-words.md`, `references/style-abbreviations.md`, `references/style-global.md`, `references/style-inclusive.md`.
- When shaping structure or media: `references/style-structure.md`, `references/style-procedures.md`, `references/style-blocks.md`, `references/style-images.md`.
- When checking formatting: `references/style-format.md`, `references/style-punctuation.md`, `references/style-numbers.md`.
- When documenting technical text: `references/style-code.md`, `references/style-cli.md`, `references/style-examples.md`, `references/style-ui.md`, `references/style-links.md`.
- When reviewing claims or APIs: `references/style-claims.md`, `references/style-api.md`.
- Live-guide provenance: `references/style-sources.md`. For a single-word question, quote the matching guidance from `assets/google-word-list.tsv` and stop.

## Verify

Run `node scripts/style-lint.mjs <changed paths>`, then hand-check non-Markdown text. ERROR findings block completion; WARN findings need correction or explanation; INFO findings need judgment. Run `--self-test` after changing a lint rule. Use `scripts/refresh-word-list.mjs --dry-run` only to check word-list drift; it fetches the live guide without writing.

Finish when the approved docs pass fact, link, safety, structure, and style checks. Name any unverified claims or residual findings. Route code-only research to `octocode-research` and skill folders to `octocode-skills`.
