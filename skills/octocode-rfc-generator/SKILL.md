---
name: octocode-rfc-generator
description: "Use when you need an RFC, design doc, architecture proposal, migration plan, implementation plan, or research-backed technical decision before coding. Leverages Octocode local/GitHub/npm/binary tools via MCP or CLI to gather evidence, compare alternatives, map blast radius, and produce a validated RFC with a practical implementation plan."
---

# Octocode RFC Generator

Use this skill when a change needs **thinking before coding**: architecture choices, migrations, cross-package changes, risky refactors, implementation plans, or formal RFC/design docs. The output is evidence-backed and actionable, not a brainstorm.

Octocode transport reference: read `references/octocode.md` when choosing, installing, or explaining Octocode MCP vs CLI usage.

Core flow:

```text
UNDERSTAND → RESEARCH → COMPARE OPTIONS → WRITE RFC / PLAN → VALIDATE → DELIVER
```

For trivial one-file edits with no design choice, skip RFC mode and route to `octocode-engineer`.

## How to run this skill

1. **Frame the work.** Read `references/workflow.md` first, before researching — it defines mode selection (RFC / Plan / Decision / Migration / Validation), brainstorming handoff intake, the understand checklist, option comparison, claim ledger, hard gates, validation, and delivery format.

2. **Gather cited evidence.** While researching current state, prior art, packages, history, or binaries, follow `references/research-playbook.md` for the exact MCP/CLI tool map per evidence type, the per-scenario research tracks, evidence-citation rules, and recovery moves when a search comes up empty.

3. **Write the document.** When producing a full RFC, adapt the body sections (Summary through Prior Art) from `references/rfc-template.md`. After drafting the body, append the closing sections from `references/rfc-implementation.md` (Unresolved Questions, Future Possibilities, References, Implementation Plan). Replace all placeholders/examples and drop unused sections. For a lighter implementation plan, use the trimmed plan skeleton in `references/workflow.md` instead of the full template.

## Non-negotiables

- Do not guess facts that tools can verify; cite local claims with `file:line` and external claims with a GitHub path/line or PR/commit link.
- Always compare at least two alternatives unless the user explicitly asks for a single implementation plan.
- Order implementation steps by dependency, not preference; avoid time estimates.
- Default output location when saving is approved: `.octocode/rfc/RFC-{meaningful-name}.md`. Ask before saving; otherwise keep the document in chat.
- When changing this skill, run `scripts/eval-rfc.mjs --self-test` and smoke prompts in `evals/prompts.md`.
