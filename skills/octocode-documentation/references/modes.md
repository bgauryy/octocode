# Modes, page types, ADRs, and agent instruction files

Load when the deliverable is unnamed, a human page needs its type, or for an ADR, `AGENTS.md`, or `CLAUDE.md`.

## Mode

| The request sounds like | Mode |
|---|---|
| AGENTS.md, CLAUDE.md, nested agent instructions, "rules the agent keeps missing"; audience is coding agents (link human pages, don't inline them) | agent-docs |
| document X, README, tutorial, how-to, API docs, runbook, onboarding | human-docs |
| decision, trade-off, alternatives, "why this over that" | adr |
| document the whole codebase, one page per package | codebase-pack |
| copyedit, style guide, tone, wording, sentence case | style-pass |

- One primary mode. Two named deliverables are two gated targets, not one blended page.
- A named file in a style-pass carries its own write approval. A page that leaves style-pass is classified by deliverable.

## Page type ([Diátaxis](https://diataxis.fr/))

| Signal | Type | Pattern |
|---|---|---|
| New to X; first success; walk me through | Tutorial | verb title; Goal → Prerequisites → steps with visible results → outcome; minimal theory |
| How do I…; known task | How-to | task title; Goal → assumptions → steps → expected result; no essays |
| Params, endpoints, flags, schema | Reference | name the thing; consistent entries (name, meaning, defaults, links) |
| Why; trade-offs; how it works | Explanation | concept title; Context → idea → alternatives → perspective; no procedures |

- No API tables mid-tutorial; no essays labeled as reference.
- Codebase-pack: write in Diátaxis order (index, reference, how-to); verify each file before the next.

## ADR

Expensive-to-reverse choices include stack, schema, auth, API style, and infra. Skip obvious code, prototypes, and restated implementation.

- Look for conventions in `<workspace>/docs/adr/`, `<workspace>/docs/decisions/`, `.adr-dir`, adr-tools, and MADR; match location, numbering, headings, and markup.
- IF conventions conflict → THEN surface the conflict; don't invent a second scheme.
- Sections: Status, Date, Context, Decision, Alternatives considered, Consequences.
- Lifecycle: `PROPOSED → ACCEPTED → (SUPERSEDED | DEPRECATED)`. Keep Status accurate.
- Link the ADR from `AGENTS.md` and architecture docs when agents reopen the debate. One screen long.
- Verify: convention matched (or default justified); ≥1 alternative or explicit none; no secrets.

## Agent instruction files

Spec: [agents.md](https://agents.md/). Exceed 100 lines only when the requester needs the detail.

- The closest nested `AGENTS.md` wins; nested files are shorter deltas.
- Workflow: 1. Add `<workspace>/docs/` to the inventory. 2. Draft as an index. 3. Verify every linked path and command exists.
- Sections, only where they add non-obvious value: package manager (one line); Commands table (task → command, file-scoped test or lint when available); External References table (need → path); Key Conventions (rules that prevent likely mistakes).
- For Claude, symlink `CLAUDE.md` to `AGENTS.md`.
- Headings, bullets, tables. Omit welcome text, skill lists, linter-config restatements, README dumps, and code blocks beyond a one-line command.

Next: outline gate → `references/write-verify.md`; wording → `references/style-pass.md`.
