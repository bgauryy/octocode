---
name: octocode-rfc-generator
description: "Use when the user needs an RFC, design doc, architecture proposal, migration plan, implementation plan, or research-backed technical decision before coding. Trigger for cross-package changes, risky refactors, option comparisons, blast-radius mapping, or a written plan with citations."
---

# Octocode RFC Generator

Use this skill when a change needs **thinking before coding**: architecture choices, migrations, cross-package changes, risky refactors, implementation plans, or formal RFC/design docs. The output is evidence-backed and actionable, not a brainstorm.

Octocode transport reference: read `references/octocode.md` when choosing, installing, or explaining Octocode MCP vs CLI usage.

Core flow:

```text
UNDERSTAND → RESEARCH → COMPARE OPTIONS → WRITE RFC / PLAN → VALIDATE → DELIVER
```

For trivial one-file edits with no design choice, skip RFC mode and route to `octocode-research` Change mode.

## Reference Map

- `references/workflow.md` — read first for mode selection, gates, claim ledger, validation, and delivery shape.
- `references/research-playbook.md` — when gathering current state, prior art, package, history, or binary evidence.
- `references/rfc-template.md` — when producing the main RFC body from Summary through Prior Art.
- `references/rfc-implementation.md` — when adding unresolved questions, references, future possibilities, and implementation plan.

For a lighter implementation plan, use the trimmed skeleton in `references/workflow.md` instead of the full RFC template.

## Non-negotiables

- Do not guess facts that tools can verify; cite local claims with `file:line` and external claims with a GitHub path/line or PR/commit link.
- Always compare at least two alternatives unless the user explicitly asks for a single implementation plan.
- Order implementation steps by dependency, not preference; avoid time estimates.
- Default output location when saving is approved: `.octocode/rfc/RFC-{meaningful-name}.md`. Ask before saving; otherwise keep the document in chat.
- When changing this skill, run `scripts/eval-rfc.mjs --self-test` and smoke prompts in `evals/prompts.md`.

## Installation

```bash
npx octocode skill --name octocode-rfc-generator
```
