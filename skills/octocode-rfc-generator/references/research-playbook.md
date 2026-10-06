# Research playbook and resources

Load when the RFC needs evidence or a `RESOURCES.md`. Why: this page owns which tracks run and where provenance is recorded. `octocode-research` owns how a search runs.

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
| Closing a plan question | `octocode-research` with local, external, or history evidence; no citation means not resolved |

## Evidence rules
- Local claims need `file:line`. External code claims need a GitHub path and line, or a PR or commit link.
- Snippets are leads. Ask `octocode-research` to upgrade them before you cite them.
- Key recommendations need one supporting source and one counterpoint or rejected alternative.
- Cite decisive claims in the section that uses them. Open `RESOURCES.md` only when the source inventory has its own lifecycle.

## Recovery
| Situation | Move |
|---|---|
| No external prior art | Say so; rely on local constraints and open questions |
| Evidence conflicts | Present the conflict and a decision rule |

## `RESOURCES.md`
Write this file only when the source inventory has its own lifecycle, and write it last. Otherwise each source stays in the section that cites it. Use one table per section: Primary Sources, Local Code References, Prior Art and Related Systems, Internal Research Artifacts. Each row has `Resource | Link or path:line | Why it matters` (for prior art: the lesson, not only the name). Add `Open Research Leads` (lead, why it matters, what makes it decision-grade) and `Reproducible Search Prompts` (`{query}`, surface, purpose).
Gate: every source says why it matters; local entries use `path:line`; external entries prefer primary sources; leads stay labeled as leads; no duplicate rows.

Next: compare options in `references/rfc-template.md` with `Comparison outcome: unresolved`, then close blockers in `references/rfc-completeness.md`. Existing-code readiness → `references/rfc-prerequisites.md`. Lock goals, then use `references/rfc-kpi.md` when measurement has its own lifecycle, then `references/rfc-implementation.md`.
