Load when saving or rendering. The owning skill's `output.md` and `report-contract` own destination, accepted statuses, required sections and when to save.

<!-- Generated copies: edit skills/common/references/report-output.md, then run skills/common/build.mjs. -->
# Report output

## Artifact ownership

Use the target workspace root, never the installed skill, tool checkout or an existing `.octocode/` subdirectory. Unless the user names a destination, save authored results under `<workspace>/.octocode/<skill-name>/`, and scratch and captures under `<workspace>/.octocode/tmp/<skill-name>/`. Edits to project sources stay in place. Reuse an explicitly selected result for revisions; give a new analysis its own path so it cannot overwrite earlier evidence.

Build copies are not evidence: record the build's digest and version in the run, and remove copied binaries and `node_modules` once the run is final and will not resume. Keep raw evidence and machine-readable records complete. A brief or visual may select a view, but must identify that selection and link to the full records. Preserve Unicode, long values, failed cases, citations, schema fields and executable continuations; disclose terminal limits and unread scope. Rendering reads the saved snapshot and leaves its source intact.

## Report header

For the bundled catalog or renderer, use YAML `status` and `question`, then a `#` title and the answer. Otherwise follow the requested format. Add confidence or pending-work fields only when useful to the receiver; never repeat frontmatter as a visible metadata block. Use the user's timezone for dated filenames.

## Choose the format

Author Markdown; the template supplies presentation. Apply the [format and content rules](presentation.md), then select the syntax below.

| Format / syntax | Use when / why |
|---|---|
| Paragraphs, `##`, **bold**, *emphasis*, `inline code` | Conclusion and evidence; scannable hierarchy, restrained emphasis. |
| Bullets / numbered lists / `- [ ]` tasks | Independent facts / simple ordered steps / checks. Prefer Mermaid when steps branch, loop or hand off; avoid deep nesting. |
| Markdown table | Options, values, criteria, findings; explicit units, consistent columns. |
| Links, reference links, `[^note]` | Evidence beside claims; secondary detail in footnotes. `path/file.ts:42` links to code. |
| `> quote` / `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]` | Exact attribution / aside, recommendation, requirement, risk, hazard. |
| Language-tagged fences; `diff` / `json` / `csv` | Runnable examples / changes / records / portable tables; displayed, not executed. |
| `![Meaningful description](https://example.com/figure.png)` | Screenshots/figures: describe the evidence in alt text; place takeaway, source and capture context nearby. Local images inside the workspace are embedded, resolved from each Markdown file; encode spaces as `%20`. HTTPS images need network access. |
| External content / `<iframe>` | Iframes are escaped as text, never loaded. Link to the external demo/video/dashboard; optionally use a captioned Markdown image preview. State what opens; never put embed HTML in `graph`/`chart`. |
| `<details>` with `<summary>` | Optional long evidence/examples; keep conclusions/actions visible. Exception to no authored HTML. |
| `mermaid` → `flowchart` | Decisions, dependencies, recovery; label branches, separate concerns. |
| `mermaid` → `sequenceDiagram` / `stateDiagram-v2` | Actor interactions / states and transitions. |
| `mermaid` → `erDiagram` / `classDiagram` | Relationships and cardinality / types and inheritance. |
| `mermaid` → `gantt` / `timeline` / `journey` / `gitGraph` / `mindmap` | Schedule / chronology / user stages / branching history / hierarchy. |
| `mermaid` → `pie` / `xychart` / `quadrantChart` / `sankey-beta` | Proportions / trends / two-axis trade-offs / weighted flows; sourced values. Prefer chart widgets for exact-value tables and interactive legends. |
| `mermaid` → `block-beta` / `architecture-beta` / `C4Context` | Block placement / service topology / system context when flowchart layout obscures structure. |
| `mermaid` → `kanban` / `packet-beta` / `requirementDiagram` | Work stages / bit fields / requirement traceability. For advanced families and exact keywords, read the [complete Mermaid chooser](report-mermaid.md), then Render check. |
| `graph` JSON | Network inspection, search, navigation; Mermaid for small fixed explanations. |
| `chart` JSON: `bar` / `line` / `pie` | Measured categories or before/after / ordered observations / parts of a whole. Keep order and units explicit; bars for precise comparisons. |

## Visual and data contracts
- Special fences: `mermaid`, `graph`, `chart`. Before graph/chart JSON, read [widget schemas and examples](report-widgets.md), which own fields, allowed types and data constraints; no library options, executable code, or duplicate authored data tables.
- Use measured data with sources and units; label estimates and unknowns. Explain each visual's takeaway nearby; use labels beyond color and Mermaid `accTitle`/`accDescr` where supported. No invented metrics or decorative graphs.
- The template owns fonts, layout, themes, search, zoom, export and fallbacks. Do not generate CSS, JavaScript, SVG, Mermaid initialization directives or a custom viewer. For an interactive graph of files, functions, layers and flows, use `octocode-graph` when installed.
- Keep Markdown as editable source and HTML as the viewing artifact; provide JSON/CSV separately when machine reuse matters. Apply [evidence rules](evidence.md) and [communication protocols](protocol-catalog.md#catalog).

## Render and retrieve

After every save, [check the report](report-render.md#check-each-save) before delivering it. When HTML was requested or the offer accepted, [render and retrieve](report-render.md#render-and-retrieve) it.
