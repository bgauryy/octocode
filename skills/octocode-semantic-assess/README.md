# Semantic assessment skill

Small entry point for the pure `octocode semanticAssess` tool. The live query schema owns the protocol and primitive contract; this skill adds routing guidance and optional examples.

```sh
octocode scheme semanticAssess --view query --compact
octocode semanticAssess --input request.json --compact
```

Each semantic query is `{reasoning,resources:[{id,context}],questions:[{id,question}]}`: every question sees every resource, results carry correlation IDs, and each resource is captured once. A matrix has at most 25 cells. Root `queries[]` batches independent semantic queries whose cross-products must stay separate. The runtime evaluates bounded pages of large resources without hiding page-local answers. Runtime configuration supplies the Jev model and credentials. Use a judgment only when it can change an unresolved next action; exact checks and settled decisions need no semantic call.

- [Setup and protocol](references/ojql.md)
- [Prompt workflows](references/jev-workflows.md)

The former standalone clients, route policies, and source readers have been retired. The runtime executes and sanitizes context, returning answers and coverage metadata without retrieved bodies. Evidence selection and follow-up actions belong to the caller; the skill has no executable client or duplicated schema.
