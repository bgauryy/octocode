# Modes

Load when choosing what to produce before research or drafting.

Pick one primary mode (combine only if the user asks for multiple deliverables):

| Mode | Deliverable | Signals |
|------|-------------|---------|
| agent-docs | `AGENTS.md`, nested agent instructions, `CLAUDE.md` symlink | AGENTS.md, CLAUDE.md, agent conventions |
| human-docs | README, tutorial, how-to, reference, explanation, runbook, onboarding | document X, README, API docs, runbook |
| adr | Architecture Decision Record | decision, trade-off, why we chose |
| codebase-pack | Multi-file docs set | document the whole codebase / generate all docs |

## Audience

- Coding agents → agent-docs first; human pages stay linked, not inlined.
- Developers / operators / newcomers → human-docs; classify Diátaxis type next.
- Future maintainers deciding again → adr.

## Next

- When researching facts, read `references/evidence-research.md`.
- When writing human-docs, read `references/diataxis.md`.
- When writing agent files, read `references/agents-md.md`.
- When recording a decision, read `references/adr.md`.
- Before WRITE, read `references/agent-readable.md`.
- For outline/write/verify, read `references/write-verify.md`.
- IF codebase-pack and the heavy writer exists → THEN hand off to `octocode-documentation-writer`.
