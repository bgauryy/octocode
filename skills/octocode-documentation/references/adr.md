# Architecture Decision Records

Load when recording a significant technical decision for future humans and agents.

ADRs capture why — context, alternatives, consequences — so agents do not re-litigate settled choices.

## When

Write for expensive-to-reverse choices (stack, schema, auth, API style, infra). Skip obvious code, prototypes, and restating the implementation.

## Convention first

Inspect existing ADR folders/tools (e.g. `docs/adr/`, `docs/decisions/`, `.adr-dir`, adr-tools, MADR). Match location, numbering, headings, and markup.

IF conventions conflict → THEN surface the conflict; do not invent a second scheme.
IF none exist → THEN use `docs/decisions/ADR-NNN-short-title.md`.

## Required sections

Status, Date, Context, Decision, Alternatives considered, Consequences.

Lifecycle: `PROPOSED → ACCEPTED → (SUPERSEDED | DEPRECATED)`. Do not delete old ADRs; supersede with a new one.

## Agent wiring

- Link the ADR from `AGENTS.md` and architecture docs when agents reopen the debate.
- Keep short enough to scan in one screen; no pasted implementations (see `agent-readable.md`).
- Keep Status accurate; a stale Accepted is worse than no ADR.

## Verify

Convention matched (or default justified); sections present; ≥1 alternative or explicit none; no secrets; linked from the docs index when relevant.
