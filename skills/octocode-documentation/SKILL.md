---
name: octocode-documentation
description: "Use when docs are missing, wrong, stale, or badly written, or need a copyedit against the Google style guide: README, API reference, runbook, CONTRIBUTING, changelog, onboarding, AGENTS.md/CLAUDE.md, ADR, migration guide, Diátaxis or whole-codebase restructure, docstrings, alt text, prose linting. Not for code, commits, or marketing copy. Code investigation → octocode-research; SKILL.md folders → octocode-skills."
---

# Octocode Documentation

Evidence-backed docs for humans and agents, written to the Google developer documentation style guide. Classify first. Gate writes. Prefer durable cross-refs over code dumps.

## Flow

`UNDERSTAND → RESEARCH → CLASSIFY → OUTLINE GATE → WRITE → STYLE → VERIFY`

UNDERSTAND names the deliverable, audience, and target paths. Compress when targets and type are named. Expand when claims need verification. A copyedit request starts at STYLE. A single-term question ("is *allows you to* okay?") is answered straight from `assets/google-word-list.tsv` — quote the guidance and stop.

## Rules

- Verify claims in the repository before asserting them. Invented commands, paths, APIs, and env vars are the one unrecoverable failure — omit or mark "Not verified in repo" instead.
- Pick one mode and load its routes before writing.
- Gate creates and overwrites unless the user approved the targets this turn; a copyedit of a named file is approved by the request itself. Touch only the files they named — propose the rest.
- Apply the style defaults in `references/style-index.md` to every line you write or edit, and name the rule when you change someone else's wording.
- A style pass changes wording, not claims; a fact change goes back to RESEARCH.
- `AGENTS.md` is an index of links and non-obvious rules, not a content dump.
- Prefer durable pointers (module path, contract name, doc link) over line numbers and pasted code.
- One Diátaxis type per page; link siblings instead of mixing.
- IF the project documents its own style guide, or the repository already applies a convention consistently → THEN follow it and report the conflict instead of adding a second scheme.

Stop when: outline gate awaits answer; write+style+verify finishes; a word-list lookup answered the question; a missing fact makes the doc dishonest to write (otherwise mark "Not verified in repository" and continue); conventions conflict; user cancels.

## Routes

- Read `references/modes.md` when choosing mode or audience.
- Read `references/evidence-research.md` when gathering or verifying repository facts.
- Read `references/diataxis.md` when writing or reviewing human-docs.
- Read `references/agents-md.md` when writing or updating agent instruction files.
- Read `references/adr.md` when recording a decision.
- Read `references/agent-readable.md` before WRITE (cross-refs, density, durability).
- Read `references/style-index.md` at STYLE, or whenever wording, formatting, or terminology is in question — it maps every Google style guide topic to the reference that owns its rules; for a review someone else acts on, read `references/style-review.md`.
- Read `references/write-verify.md` for outline gate, write steps, and verify checklist.

## Related

- Pure code or repository evidence with no docs deliverable → `octocode-research`; authoring a `SKILL.md` → `octocode-skills`.
- Full multi-file pack → `octocode-documentation-writer` if installed; otherwise work file by file, gating each write.
- Unclear mode → ask once: agent-docs / human-docs / adr / codebase-pack / style-pass. No Octocode → host search tools.

## Scripts
- `scripts/style-lint.mjs <paths>` — Markdown only; ERROR gates, WARN is mechanical, INFO needs judgment. One finding per rule per line, `--max-per-rule` caps per file (default 20), `<!-- style-lint: ignore-file -->` skips a file found by recursion. Exit 1 on ERROR, or on WARN with `--strict`; 2 on bad usage. Docstrings, HTML, and UI strings stay hand-checked.
- `scripts/refresh-word-list.mjs --dry-run` — rebuilds `assets/google-word-list.tsv` from `developers.google.com` (it fetches even with `--dry-run`, and refuses to write a short parse).
- `scripts/style-lint.mjs --self-test` — lints built-in good/bad fixtures; run it after editing a rule so a gate can't go inert.
