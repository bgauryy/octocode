# Skill review rules

Load when interpreting or fixing review findings — after running `scripts/skill-review.mjs`. Why: map each code to the exact gap.

## ERROR (exit 1)

| Code | Meaning |
|------|---------|
| `frontmatter-missing` | `SKILL.md` starts with YAML frontmatter |
| `name-mismatch` | frontmatter `name` equals the folder name |
| `description-missing` | non-empty `description` |
| `description-too-long` | `description` ≤1024 chars |
| `missing-route` | every routed `references/*`, `scripts/*`, and `scheme/*` path exists |
| `link-outside-skill` | no dependency on a file outside the folder — no `../dir/file`, `~/`, `file://`, or absolute path (a bare `../dir` argument is fine, as is a path carrying a `<placeholder>`) |
| `octocode-contract-stale` | use current public Octocode tool names and `octocode skill install/list/info` command forms |
| `lobby-*-convention` | declare the required `tools`, `related-skill`, `output`, and actionable `routes` lines below the H1 |
| `scheme-contract` | every `scheme/` entry is a flat `.json` file containing one valid top-level object |
| `unused-file` | every shipped file is reachable from `SKILL.md`, `README.md`, or another used file; remove development-only metadata, probes, duplicates, and dead weight |

## WARN codes → assess

| Code | Fix |
|------|-----|
| `description-trigger` | lead with `Use when <trigger>` |
| `lobby-long` | keep `SKILL.md` lean; move depth into one-concept refs |
| `readme-missing` | add `README.md`: overview, capabilities, how it works, install |
| `reference-h1` | one short H1 per reference |
| `reference-long` | inspect for mixed concepts or duplication; retain a coherent procedure when splitting adds navigation cost |
| `orphan-reference` | route it from `SKILL.md` or another reference, or delete it |
| `lobby-reference-unlisted` | confirm a clear route from the lobby or an owned index; add a missing route without duplicating the catalog |
| `lobby-script-unlisted` | confirm an agent-facing usage route; internal helpers can remain behind a routed command |
| `lobby-workflow-missing` | show the workflow on its own line in `SKILL.md` — a `Flow:` line or a `## Workflow` heading, not trailing mid-sentence |
| `script-unreferenced` | a library nothing imports: import it, name it, or drop it |
| `route-condition` | state when or why on the same line as the ref or script |
| `reference-entry-cue` | open the chunk with `Load when …` and `Why:` |
| `reference-dead-end` | add a next hop when execution depends on it; a complete reference needs no ceremonial closing line |
| `flow-phase-unrouted` | name each flow phase in a route or gate, or drop it from the flow |
Navigation warnings are review candidates, not mandatory formatting. Check reachability and use conditions; retain clear nested routes and complete references. Audit trails, templates, and fixtures skip entry/exit cues. A concrete directory route includes its files; write schematic paths with a placeholder such as `scripts/<hook-directory>/` so the reviewer does not require that example to ship.
## Judgment checks the script cannot make

| Check | Fix |
|-------|-----|
| Duplicate or weak prose | one owner per concept; cross-link instead of restating; use direct verbs and named objects; cut filler without losing data |
| Lobby and routed-file convention | declare tools, one related-skill, and the workspace-versus-home output decision without implying install authority; each reference/doc/script/scheme owns a coherent conditional job that changes the next action; move short shared rules to the lobby and delete wrappers/stubs |
| Output and gates | real markdown table for tabular data; complete gate sections |
| `description` quality | one `Use when`; intents, not internals; no MUST/NEVER/ONLY-skill, second `Triggers:`, or quote spam |
| Scripts and hooks | `--help` and flags; extract deterministic prose; route hook + `timeout`; a declared host scheme is really exposed |
| Portability | core commands run in a single-folder copy; optional integrations declare their dependency and setup, and pass documented tests with it absent/present |
| Artifact roots | lobby routes workspace artifacts under `<workspace>/.octocode/`, no-workspace or explicitly user-scoped artifacts under `<home>/.octocode/`, and distinguishes requested source mutations; write failures never silently switch roots |
Required: internal dependencies resolve and shipped files have a purpose. Length and lobby-listing warnings need judgment; explain retained warnings rather than expanding or fragmenting instructions mechanically. Next: use `references/skill-review.md` to rerun checks and `references/skill-anatomy.md` for design guidance.
