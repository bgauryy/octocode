Load when running rfc-trace on an RFC, reading its exit code and fields, or checking what the script cannot judge.

# RFC trace

After each draft or revision of Features, Tasks and rollout, Validation and success or Sources, and on an RFC under audit, run [rfc-trace.mjs](../scripts/rfc-trace.mjs) from any directory: `node <skill-folder>/scripts/rfc-trace.mjs <RFC.md>`. It reads the file, writes nothing and prints JSON; local links resolve from the RFC's folder, then the workspace root. Its `--help` lists the ID rules and options.

- Exit 0, `ok: true`: IDs, coverage, task order, citations and local links trace. The judgment checks below still apply.
- Exit 1: each non-empty list is a gap. Fix the RFC and rerun until exit 0, or carry the gap as a finding.
- Exit 2: the RFC is not in the [output structure](../output.md#structure) the trace reads; the error message names the missing section, Tasks column or argument. Restore it and rerun; in an audit, report it as a format finding.

| Field | Fix |
|---|---|
| `undefinedIds`, `duplicateIds` | Define each ID once, as its section's table row or labelled item, or correct the reference. A token that is no RFC ID (a `P99` latency) goes in `--not-id P99` |
| `featuresWithoutTask`, `featuresWithoutCheck` | Add the task that builds the feature and the `V` that names it; without a check it is unaccepted. A feature with Priority `Won't` is outside both lists |
| `tasksWithoutServes` | Name the `F` or `P` it serves; a task serving neither is scope creep or a missing feature |
| `forwardDeps`, `cycles` | Reorder or split tasks: no task depends on a later one |
| `citationsNotInSources` | List that source in Sources, as a link |
| `brokenLocalLinks` | Fix the target; `reason` says whether the path, anchor, line, reference or footnote is missing |
| `criticalPath`, `taskGraphMermaid` | Not findings: name the path in the rollout and draw the graph per [diagrams](diagrams.md) |

The script cannot judge:

- One name per thing: each glossary term and entity defined once under its code name, never two words for it.
- Which claims decide: each one cites a source. Remote links are not fetched; open each deciding one.
- The first task is startable now: its prerequisites hold, or its blocker is a `Blocking:` question.
- Each `V` observes the after state of the feature it names.

A gap is a finding: add the row or state why it stays out of scope.
