# Visual design

Load when the map looks cluttered, a view misleads, or the template itself needs changes. Why: architecture diagrams fail in known ways (hairballs, mixed abstraction levels, and undirected or unexplained edges), and each view in the template answers one question.

## Which view answers what

| View | Answers | Design choice |
|---|---|---|
| Layers | "What calls what, and does the layering hold?" | Swimlanes in caller → callee order, barycenter ordering to cut crossings, edge ports spread per card, upward edges red dashed, and runtime kinds drawn bold over faint imports |
| Graph | "What clusters, and what is central?" | Force layout pinned to layer bands, node area ∝ loc, ■ stores and ◆ externals, and file-level drill-down via *Show files* |
| Flows | "What happens at runtime for scenario X?" | UML-style sequence diagram with numbered messages, step/play controls, and *Map* overlay on the layers view |
| Matrix | "Where are cycles and violations at scale?" | Dependency structure matrix ordered by layer: marks below the diagonal are upward or cyclic, and it stays readable at hundreds of nodes |
| 3D | "What does the whole system look like?" | Layers stacked vertically, with particles showing direction. Use it for orientation only; read details in 2D |

## Rules the template follows (keep them when editing)

- One abstraction level per view: the Containers/Components toggle follows the C4 model. Never mix files and packages in the same view; files appear only in focus mode.
- Encode meaning in color twice: layer color on the card bar and node fill, and edge color by kind. Red and orange are reserved for violations and cycles.
- Progressive disclosure: hovering a node dims everything but its neighborhood, clicking opens evidence, search centers the result, and layer or edge-kind toggles reduce clutter.
- Every visual claim links to source: evidence entries open the file in the editor (`meta.editor`) or copy `path:line`.
- The file stays self-contained apart from two pinned CDN scripts (d3@7.9.0 and 3d-force-graph@1.80.0). State lives in the URL hash so a view can be shared by link. The light/dark theme uses CSS variables.

## Decluttering recipe

1. Fold sub-components that never appear in a flow (`mergeInto`), or switch to Containers.
2. `exclude` fixtures, generated code, vendored copies, and benchmark corpora.
3. Hide `type-import` (off by default) or `import` in the Connections list to see runtime edges only.
4. Past about 60 components, lead with the Matrix view and name the heaviest clusters in the summary.

Test a template change by opening it raw (it shows a demo model) and with a real model through `scripts/render.mjs`, in light and dark, at narrow and wide window sizes.
