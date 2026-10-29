Load when saving or rendering. The owning skill's `output.md` and `report-contract` own destination, accepted statuses, required sections and when to save.

<!-- Generated copies: edit skills/common/references/report-output.md, then run skills/common/build.mjs. -->
# Report output

## Report header

Start with YAML `status` and `question`; `confidence` is optional, `next` only for pending work. Then one `#` title and a BLUF answer with its main limitation. Use the user's timezone for dated filenames. Keep required frontmatter for the catalog only; never repeat it as a metadata block in the document.

## Choose the format

Author Markdown; the template supplies presentation. Follow the [shared visual-first, content-only rules](presentation.md): visualize flows and logic, compare values graphically, retain exact evidence, and omit probe output and process metadata.

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
| `mermaid` → `pie` / `xychart-beta` / `quadrantChart` / `sankey-beta` | Proportions / trends / two-axis trade-offs / weighted flows; sourced values. Prefer chart widgets for exact-value tables and interactive legends. |
| `mermaid` → `block-beta` / `architecture-beta` / `C4Context` | Block placement / service topology / system context when flowchart layout obscures structure. |
| `mermaid` → `kanban` / `packet-beta` / `requirementDiagram` | Work stages / bit fields / requirement traceability. For advanced families and exact keywords, read the [complete Mermaid chooser](report-mermaid.md), then Render check. |
| `graph` JSON | Network inspection, search, navigation; Mermaid for small fixed explanations. |
| `chart` JSON: `bar` / `line` / `pie` | Measured categories or before/after / ordered observations / parts of a whole. Keep order and units explicit; bars for precise comparisons. |

## Visual and data contracts
- Special fences: `mermaid`, `graph`, `chart`. Before graph/chart JSON, read [widget schemas and examples](report-widgets.md), which own fields, allowed types and data constraints; no library options, executable code, or duplicate authored data tables.
- Use measured data with sources and units; label estimates and unknowns. Explain each visual's takeaway nearby; use labels beyond color and Mermaid `accTitle`/`accDescr` where supported. No invented metrics or decorative graphs.
- The template owns fonts, layout, themes, search, zoom, export and fallbacks. Do not generate CSS, JavaScript, SVG, Mermaid initialization directives or a custom viewer. For an interactive graph of files, functions, layers and flows, use `octocode-graph-view` when installed.
- Keep Markdown as editable source and HTML as the viewing artifact; provide JSON/CSV separately when machine reuse matters. Apply [evidence rules](evidence.md) and [communication protocols](protocol-catalog.md#catalog).

## Render and retrieve

Apply [HTML delivery](presentation.md#html-delivery) after completing the task. Saving Markdown does not request HTML. The commands below apply when HTML was requested or the user accepted the offer.

1. Run `node <skill-folder>/scripts/render-report.mjs report.md --out report.html --json --strict`. Exit 2 means fix reported validation issues and rerun; no new HTML was written, so do not deliver an older file as the result.
2. Success reports `render: unverified`. Open the page and run **Render check** until Ready; it checks diagrams, widgets and image loading. Inspect alt text, readable sizing and nearby source links; check other relevant links too. Correct failures or disclose unverified delivery. Browser unavailable: say rendering was not verified; generation alone is not visual verification.
3. Rerender after edits; deliver the HTML link and useful source/data links. `--open` is opt-in. A folder renders a navigable library; `.octocode/` Markdown defaults to sibling HTML, other destinations come from the command result.
4. Discover with `node <skill-folder>/scripts/render-report.mjs <workspace>/.octocode/<skill-name> --manifest --query "topic"`; follow returned `next` arguments for more than 20 records. Use `--all` only for file processing, not model context.
5. Read relevant Markdown only; never load generated HTML just to review content. Use compact `--json` results. Do not equate artifact bytes with token savings. Viewer changes also require narrow-screen, light/dark, keyboard, long-table and print checks; disclose missing checks.
