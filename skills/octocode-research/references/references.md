# Sources and skill maintenance

Load when checking why a rule exists or before editing this skill. Installed tool schemas control accepted fields; re-verify sources when behavior may have changed.

| Claim area | Primary source |
|---|---|
| Octocode fields, limits, availability | `scheme`, `scheme <name> --view query --compact`, then implementation + tests |
| GitHub search scope, caps | [REST search](https://docs.github.com/en/rest/search/search) |
| Refs, pagination, rate limits | [Contents](https://docs.github.com/en/rest/repos/contents), [pagination](https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api), [best practices](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api) |
| PRs, commits | [Pull requests](https://docs.github.com/en/rest/pulls/pulls), [Commits](https://docs.github.com/en/rest/commits/commits) |
| npm / PyPI / Go | [npm registry](https://github.com/npm/registry/blob/main/docs/REGISTRY-API.md), [PyPI JSON](https://docs.pypi.org/api/json/), [Go API](https://pkg.go.dev/v1/api) |
| Semantics | [LSP 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) |
| Skill structure | [Agent Skills specification](https://agentskills.io/specification) |

Exact source plus matching tests establish implementation claims; official docs establish API contracts; a README or issue alone never establishes runtime behavior.

## Editing this skill
`GOAL + failing check → BASELINE → smallest coherent change → MEASURE → ACCEPT | REVERT`. Run before and after:
```bash
node scripts/check-description.mjs
node scripts/check-guidance.mjs --self-test --examples
```
then the `octocode-skills` folder review with zero errors. Never edit a check to match the text. These are offline contract checks; effectiveness claims need real tasks with graded outcomes (`octocode-eval-benchmark`).
