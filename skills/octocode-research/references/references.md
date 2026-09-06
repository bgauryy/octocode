# Primary sources

Load when checking why a tool constraint or research rule exists. Verify current sources when behavior may have changed; the installed tool schema controls accepted Octocode fields.

| Claim area | Primary source | What to verify |
|---|---|---|
| Octocode invocation and fields | Runtime `tools --json --compact` and `tools <name> --scheme --json` | availability, operations, relations, limits; implementation and tests in the matching checkout |
| GitHub search | [REST search](https://docs.github.com/en/rest/search/search) | indexed scope, incomplete results, result caps, search qualifiers |
| Exact file/ref reads | [Repository contents](https://docs.github.com/en/rest/repos/contents) | ref semantics and file/directory responses |
| Provider pagination and efficiency | [Pagination](https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api) and [REST best practices](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api) | provider links, conditional requests, rate limits |
| PR and commit evidence | [Pull requests](https://docs.github.com/en/rest/pulls/pulls) and [Commits](https://docs.github.com/en/rest/commits/commits) | identities, patches, pagination, comparison bounds |
| npm lookup/discovery | [npm registry API](https://github.com/npm/registry/blob/main/docs/REGISTRY-API.md) | exact package metadata, search text/size/from, published provenance |
| Semantic capabilities | [Language Server Protocol](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) | negotiated providers, requests, locations; actual server support can differ |
| Skill structure | [Agent Skills specification](https://agentskills.io/specification) | frontmatter, focused references, progressive disclosure, standalone scripts |

Exact local source and matching tests establish implementation claims. Official docs establish supported API contracts. Neither a repository README nor an issue report alone establishes runtime behavior. Record source version/ref, access date where useful, contrary evidence, and checks not run.

Do not claim a skill is universally best from static checks. Rate contract correctness, routing, evidence, completeness, and measured scenario outcomes separately; broader agent effectiveness needs an independent task corpus.

Next: choose evidence with `references/algorithm.md`; assess external provenance with `references/workflow-external.md`; validate skill changes with `references/improve-loop.md`.
