# Jev CLI skill

Optional ten-line entry point for the pure `octocode jev` tool. The live query schema owns protocol and primitive guidance; this skill adds CLI discovery and links to optional examples.

```sh
octocode scheme jev --view query --compact
octocode jev --input request.json --compact
```

Prefer `{reasoning, resources:[{id,context}], questions:[{id,question}]}` for a shared question set; every question sees every resource, result rows carry both IDs, and each resource is captured once. A matrix has at most 25 cells, 25 resources, and five questions. Use `queries[]` only for independent `{reasoning, context, question}` pairs whose cross-product would be wrong. Split huge browser/files into bounded resources and page successive matrices until every chunk is judged. Runtime configuration supplies the model and credentials. Use a judgment only when it can change an unresolved next action; exact checks and settled decisions need no Jev call.

- [Setup and protocol](references/ojql.md)
- [Prompt workflows](references/jev-workflows.md)

The former standalone clients, route policies, and source readers have been retired. The runtime executes and sanitizes context, returning answers and coverage metadata without retrieved bodies. Evidence selection and follow-up actions belong to the caller; the skill has no executable client or duplicated schema.
