# Modes

Load when the request doesn't name its deliverable. Why: the mode fixes the route order in `SKILL.md`, so classifying wrong costs the whole pass. This file owns the signals and the audience call; `SKILL.md` owns each mode's deliverable and route order.

## Signals

| The request sounds like | Mode |
|------|-------------|
| AGENTS.md, CLAUDE.md, nested agent instructions, "rules the agent keeps missing" | agent-docs |
| document X, README, tutorial, how-to, API docs, runbook, onboarding | human-docs |
| decision, trade-off, alternatives, "why this over that" | adr |
| document the whole codebase, generate the docs set, one page per package | codebase-pack |
| copyedit, style guide, tone, wording, sentence case, "does this read well" | style-pass |

Pick one primary mode. Combine only when the request names two deliverables — then gate them as two targets, not one blended page.

## Audience

- Coding agents → agent-docs first; human pages stay linked, not inlined.
- Developers, operators, newcomers → human-docs; classify the Diátaxis type next.
- Future maintainers deciding again → adr.
- Docs that exist and are factually right but read badly → style-pass; wording only, no new content.

## Ties and edge cases

- IF the request names an existing file and asks only about wording → THEN style-pass, and the named file carries its own write approval.
- IF the page needs a fact nobody has verified → THEN it isn't style-pass; classify by deliverable and research first.
- IF signals tie, or the deliverable stays unclear after one read → THEN ask once, listing the five modes, and don't start writing meanwhile.

## Next

- Facts to gather or verify → `references/evidence-research.md`.
- human-docs type → `references/diataxis.md`; agent files → `references/agents-md.md`; a decision → `references/adr.md`.
- Before WRITE, whatever the mode → `references/agent-readable.md`, then `references/write-verify.md`.
- style-pass → `references/style-index.md`; a review someone else acts on → `references/style-review.md`.
- IF codebase-pack → THEN plan the file set, gate it once, then write file by file in Diátaxis order (index, then reference, then how-to), verifying each before the next.
