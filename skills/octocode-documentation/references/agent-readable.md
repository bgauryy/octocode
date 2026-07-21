# Agent-Readable Docs

Load before WRITE for any audience. Why: agents navigate by links and stable names; verbose code dumps and line citations rot fast.

## Density

- Lead with the fact or rule; cut intros, slogans, and restated README prose.
- One idea per bullet; tables for command/path maps.
- Prefer short pages that link deeper pages over one long page.

## Cross-refs

- New or updated pages should link related docs (parent index, sibling how-to/reference, ADRs).
- `AGENTS.md` needs an External References (or Docs) table to the real sources of truth.
- Use repo-relative paths in backticks or markdown links — not vague "see docs".
- When two pages overlap, keep one owner and link the other.

## Durable over brittle

- Point to modules, packages, entry files, and doc pages — not `file:line` in standing docs.
- Describe contracts and behavior ("token exchange under `packages/mcp-host` auth services") instead of pasting implementations.
- Name manifest scripts; do not embed large shell programs.
- IF code and docs disagree → THEN trust code for facts, fix or flag the doc, and avoid ephemeral details.

## No code dumps

- Skip multi-block source pastes, full configs, and long JSON/YAML in docs.
- Allowed: one short command or signature when copy-paste is required; otherwise link the file.
- Keep examples minimal; point to tests or source for the full story.

## Agent comprehension check

Before finish: links + bullets should answer "where do I look?" and "what must I not break?". IF either answer is missing → THEN add refs or cut noise.
