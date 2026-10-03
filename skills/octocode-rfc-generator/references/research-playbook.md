# Research playbook and resources

Load when the RFC needs evidence or a `RESOURCES.md`. This file owns the RFC evidence plan and provenance. `octocode-research` owns how research runs: ask it for surfaces, citations, confidence, source inventory, and gaps.

## Run only the tracks that matter
| Scenario | Research tracks |
|---|---|
| Existing-system change | Local current state and affected scope; external prior art if options are unclear |
| Worth-building, option space, or criteria unresolved | `octocode-brainstorming` first; otherwise `octocode-research` |
| Greenfield choice | External prior art, package and repository comparison; local constraints if a repository exists |
| Migration | Local current state, contracts and data flows, external migration examples |
| Library or package adoption | npm metadata, repository source, local integration points |
| Refactor plan | Local structure, LSP references and callers, AST duplication and smell checks |
| RFC validation | Map each claim to evidence; mark confirmed, likely, or uncertain |
| Closing `IMPLEMENTATION.md` questions | `octocode-research` with local, external, or history evidence; no citation means not resolved |

## Evidence rules
- Local claims need `file:line`. External code claims need a GitHub path and line, or a PR or commit link.
- Snippets are leads. Ask `octocode-research` to upgrade them before you cite them.
- Key recommendations need one supporting source and one counterpoint or rejected alternative.
- Cite decisive claims inline in `RFC.md`, `PLAN.md`, `PREREQUISITES.md`, `IMPLEMENTATION.md`, or `KPI.md`. Put the broad inventory in `RESOURCES.md`.

## Recovery
| Situation | Move |
|---|---|
| No external prior art | Say so; rely on local constraints and open questions |
| Evidence conflicts | Present the conflict and a decision rule |

## `RESOURCES.md`
Write it last. Use one table per section: Primary Sources, Local Code References, Prior Art and Related Systems, Internal Research Artifacts. Each row has `Resource | Link or path:line | Why it matters` (for prior art: the lesson, not only the name). Add `Open Research Leads` (lead, why it matters, what makes it decision-grade) and `Reproducible Search Prompts` (`{query}`, surface, purpose).
Gate: every source says why it matters; local entries use `path:line`; external entries prefer primary sources; leads stay labeled as leads; no duplicate rows.

Next: decision mode → compare provisionally with `references/rfc-template.md`, then close blockers with `references/rfc-prerequisites.md`; acceptance → `references/rfc-kpi.md`; build → `references/rfc-implementation.md`.
