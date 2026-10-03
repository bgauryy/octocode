# Modes, page types, ADRs, and agent instruction files

Load when the request does not name its deliverable, or a human page needs its type (§ Mode, § Page type); when recording a significant technical decision for future humans and agents (§ ADR); or when creating or updating `AGENTS.md`, nested agent instructions, or a `CLAUDE.md` entrypoint (§ Agent instruction files). This file owns mode signals, audience, page type, and the ADR and `AGENTS.md` shapes; `SKILL.md` owns route order.

## Mode

| The request sounds like | Mode |
|---|---|
| AGENTS.md, CLAUDE.md, nested agent instructions, "rules the agent keeps missing" | agent-docs |
| document X, README, tutorial, how-to, API docs, runbook, onboarding | human-docs |
| decision, trade-off, alternatives, "why this over that" | adr |
| document the whole codebase, one page per package | codebase-pack |
| copyedit, style guide, tone, wording, sentence case | style-pass |

- Pick one primary mode. Two named deliverables are two gated targets, not one blended page.
- Audience: coding agents → agent-docs (link human pages, do not inline them); developers, operators, newcomers → human-docs; future maintainers deciding again → adr; correct docs that read badly → style-pass (wording only).
- IF the request names an existing file and asks only about wording → THEN style-pass; the named file carries its own write approval.
- IF the page needs an unverified fact → THEN it is not style-pass; classify by deliverable and research first.
- IF signals tie, or the deliverable stays unclear after one read → THEN ask once, list the five modes, and do not write meanwhile.

## Page type ([Diátaxis](https://diataxis.fr/))

| Signal | Type | Pattern |
|---|---|---|
| New to X; first success; walk me through | Tutorial | verb title; Goal → Prerequisites → steps with visible results → outcome; minimal theory |
| How do I…; known task | How-to | task title; Goal → assumptions → steps → expected result; no essays |
| Params, endpoints, flags, schema | Reference | name the thing; consistent entries (name, meaning, defaults, links); lookup in seconds |
| Why; trade-offs; how it works | Explanation | concept title; Context → idea → alternatives → perspective; no procedure dumps |

- One type per page; link sibling types (tutorial → reference; how-to → explanation). No API tables mid-tutorial; no essays labeled as reference.
- Outline stub: type, audience, goal, sections, out path, exclude (what belongs elsewhere).
- Codebase-pack: write in Diátaxis order (index, reference, how-to) and verify each file before the next.

## ADR

ADRs capture why — context, alternatives, consequences — so agents do not re-litigate settled choices. Write for expensive-to-reverse choices (stack, schema, auth, API style, infra). Skip obvious code, prototypes, and restating the implementation.

- Convention first: inspect existing ADR folders/tools in the user's workspace (for example, `<workspace>/docs/adr/`, `<workspace>/docs/decisions/`, `.adr-dir`, adr-tools, MADR). These are project output locations, not files bundled with this skill. Match location, numbering, headings, and markup.
- IF conventions conflict → THEN surface the conflict; do not invent a second scheme. IF none exist → THEN use `<workspace>/docs/decisions/ADR-NNN-short-title.md`.
- Required sections: Status, Date, Context, Decision, Alternatives considered, Consequences.
- Lifecycle: `PROPOSED → ACCEPTED → (SUPERSEDED | DEPRECATED)`. Do not delete old ADRs; supersede with a new one.
- Agent wiring: link the ADR from `AGENTS.md` and architecture docs when agents reopen the debate. Keep short enough to scan in one screen; no pasted implementations (see `references/evidence-research.md`). Keep Status accurate; a stale Accepted is worse than no ADR.
- Verify: convention matched (or default justified); sections present; ≥1 alternative or explicit none; no secrets; linked from the docs index when relevant.

## Agent instruction files

Spec: [agents.md](https://agents.md/). Goal: the smallest useful map for coding agents. Aim for 60 lines; exceed 100 only when the requester needs the added detail.

- Role: index of where truth lives + non-obvious gotchas only. Complements README/CONTRIBUTING — does not replace them. Closest nested `AGENTS.md` wins; nested files stay shorter than root and only add deltas.
- Workflow: 1. Inventory the project's manifests, CI, README, `<workspace>/docs/`, ADRs, SECURITY, and existing AGENTS.md. 2. Collect exact commands from those sources — do not invent scripts. 3. Draft as an index: Package Manager → Commands → External References → Key Conventions. 4. Verify every linked path and command exists.
- Required shape — use only sections that add non-obvious value: package manager (one line); Commands table (task → command), preferring file-scoped test/lint when available; External References table (need → path), which is how agents find deeper docs; Key Conventions — only rules that prevent likely mistakes.
- For a Claude entrypoint, symlink `CLAUDE.md` to `AGENTS.md` so the instructions cannot diverge.
- Content rules: headings, bullets, tables — not paragraphs. Link docs instead of copying them (see `references/evidence-research.md`). Omit welcome text, skill lists, linter-config restatements, README dumps, and code blocks beyond a one-line command.
- Verify: commands exist in manifests/Makefile/CI; every reference path exists; length within budget; nested files are deltas only.

Next: outline gate, write steps, and the full verify checklist → `references/write-verify.md`; wording and formatting rules → `references/style-pass.md`.
