# Question sets

Research sets are data: one file per question, with no hints, approach or answer.
The tool capability set groups specific questions by tool; common execution checks remain in CHECKS.md. Pick a set by what
the round compares ([COMPARISONS.md](../COMPARISONS.md)), and never pool sets in one aggregate.

| Set | Questions | Scope | Stored answers or numbers (see the [results index](../results/README.md)) |
|---|---:|---|---|
| [tools/](tools/README.md) | 17 tool files | all local CLI/MCP tools, input/output, pages, workflows and expert comparisons | each dated tool-rating campaign; assignments and source oracles stored separately |
| [github/](github/README.md) | 30 | public GitHub repositories | Octocode v20 (2026-10-29) and v18 (2026-08, 3 passes); gh-lean answers (2026-10-09); plain gh, gh + RTK, gh + Headroom as numbers only (2026-08); MCP builds and clasify A/B (2026-10) |
| [local-core/](local-core/README.md) | 30 | pinned clones listed in [local/README.md](../local/README.md); judge rubrics in `answers/` | Octocode local MCP builds, 19.1.0 and the lean shell (2026-10-10) |
| [local-goals/](local-goals/README.md) | 40 | nine pinned checkouts, `{{REPO:id}}` tokens resolved per `catalog.json` and `repositories.json`; Q33–Q40 are a Terraform slice | local MCP with and without clasify (2026-10-13) |
| local-transfer/ | 12 | same format as local-goals | clasify guidance transfer (2026-10-13) |
| local-scenarios/ | 30 | language root-cause scenarios: a goal and questions only; `repositories.json` maps each to its clone and commit | native CLI against shell exploration (2026-10-17) |

How a local round uses the local sets, flow by flow, is in [local/FLOWS.md](../local/FLOWS.md); the
repositories they need are in [local/README.md](../local/README.md).

## Rules

- **The question carries no answer.** Rubrics (`local-core/answers/`, or private storage for
  local-goals) go to the judge only, never to a runner.
- **Repository assignment lives outside the question text**, in the set's `repositories.json` or
  `catalog.json` or in the question's own named clone path. Don't rename questions or add
  repository hints to fit a runner.
- **Stored answers exist only for the questions they cover.** A new question is compared only
  among arms run in the same round.

## Add a question

Add it to the set it belongs to, with the next number, and add its row to that set's README:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

One self-contained, objectively checkable prompt. Name the repository and ref, or the clone
path, and say exactly what to report. No hints, no approach, no answer.
```
