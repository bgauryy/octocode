# Agent instruction files

Load when creating or updating `AGENTS.md`, nested agent instructions, or a `CLAUDE.md` entrypoint. Spec: [agents.md](https://agents.md/).

Goal: the smallest useful map for coding agents. Aim for 60 lines; exceed 100 only when the requester needs the added detail.

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
- External References table (need → path); this is how agents find deeper docs
- Key Conventions — only rules that prevent likely mistakes

For a Claude entrypoint, symlink `CLAUDE.md` to `AGENTS.md` so the instructions cannot diverge.

## Content rules

- Headings, bullets, tables — not paragraphs.
- Link docs instead of copying them (see `references/agent-readable.md`).
- Omit welcome text, skill lists, linter-config restatements, README dumps, and code blocks beyond a one-line command.

## Verify

- Commands exist in manifests/Makefile/CI
- Every reference path exists
- Length within budget; nested files are deltas only

Next: outline gate, write steps, and the full verify checklist → `references/write-verify.md`; wording and formatting rules → `references/style-index.md`.
