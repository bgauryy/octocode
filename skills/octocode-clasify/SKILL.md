---
name: octocode-clasify
description: "Use when clasify can make a bounded Noul, Choice, or Score judgment over supplied or unread resources that changes the next action; skip exact checks and settled decisions."
---
# Semantic assessment

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Flow: `INSPECT → SHAPE → ASSESS → VERIFY`.

Inspect `octocode scheme clasify --view query --compact` once; reuse its description and query schema while current. Run `octocode clasify --input request.json --compact` with one semantic query or `{queries:[...]}` for independent semantic queries. Each semantic query supplies `resources:[{id,context}]` and `questions:[{id,question}]`; every resource is assessed against every question. The runtime captures a resource once, groups its questions into provider requests, automatically evaluates bounded source pages, and returns correlated query/resource/question/page results. Keep each matrix at 25 cells or fewer. Read retained proof for relevant, uncertain, insufficient, partial, and errored results. The runtime selects the provider model and reports requested and resolved model identities separately.

Shape only independent matrices together. Assess them once, then verify retained candidates with exact reads or tests.

Use Choice with explicit `relevant`, `unrelated`, and `insufficient` options to scout. Use one Noul for one affirmative yes/no proposition. Use one Score for one ordered dimension with 2–10 independently described levels. Several aspects are several questions; do not hide a checklist inside one instruction or ask for free-form reasoning. Jev answers are judgments, not proof or permission: follow them with exact reads or tests before claims and changes.
In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.
For setup → [ojql.md](references/ojql.md). For optional examples → [jev-workflows.md](references/jev-workflows.md).
