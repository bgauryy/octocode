# Agent Instruction Files

Load when creating or updating `AGENTS.md`, nested agent instructions, or a `CLAUDE.md` entrypoint. Spec: [agents.md](https://agents.md/).

Goal: smallest useful map for coding agents. Target ≤60 lines; never exceed 100 unless the user insists.

## Role of AGENTS.md

- Index of where truth lives + non-obvious gotchas only.
- Complements README/CONTRIBUTING — does not replace them.
- Closest nested `AGENTS.md` wins; nested files stay shorter than root and only add deltas.

## Workflow

1. Inventory manifests, CI, README, `docs/`, ADRs, SECURITY, existing AGENTS.md.
2. Collect exact commands from those sources — do not invent scripts.
3. Draft as an index: Package Manager → Commands → External References → Key Conventions.
4. Verify every linked path and command exists.

## Required shape

Use only sections that add non-obvious value:

- Package manager (one line)
- Commands table (task → command); prefer file-scoped test/lint when available
- External References table (need → path) — REQUIRED; this is how agents find deeper docs
- Key Conventions — only rules that prevent likely mistakes

IF Claude entrypoint needed → THEN symlink `CLAUDE.md` to `AGENTS.md`. FORBIDDEN: divergent copies.

## Writing rules

- Headings, bullets, tables — not paragraphs.
- Link docs instead of copying them (see `agent-readable.md`).
- FORBIDDEN: welcome text, skill lists, linter config restatements, README dumps, code blocks beyond a one-line command.

## Verify

- Commands exist in manifests/Makefile/CI
- Every reference path exists
- Length within budget; nested files are deltas only
