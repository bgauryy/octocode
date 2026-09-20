# Jev CLI skill

Optional ten-line entry point for the pure `octocode jev` tool. The live query schema owns protocol and primitive guidance; this skill adds CLI discovery and links to optional examples.

```sh
octocode scheme jev --view query --compact
octocode jev --input request.json --compact
```

Supply `{reasoning, context, question}`: one question over an unread read-tool request `{tool, query}` or supplied `{value}`. Batch up to five independent queries, repeating context explicitly. Runtime configuration supplies the model and credentials. Use a judgment only when it can change an unresolved next action; exact checks and settled decisions need no Jev call.

- [Setup and protocol](references/ojql.md)
- [Prompt workflows](references/jev-workflows.md)

The former standalone clients, route policies, and source readers have been retired. The runtime executes and sanitizes context, returning answers and coverage metadata without retrieved bodies. Evidence selection and follow-up actions belong to the caller; the skill has no executable client or duplicated schema.
