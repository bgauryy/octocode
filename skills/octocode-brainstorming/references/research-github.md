Load when a claim turns on a repository (mechanism, failures, adoption, changes, maintainers, a package version), or when planning the next round across web and GitHub.

# GitHub research and combining it with the web

## GitHub

Repositories hold the primary artifact a web claim is about: the code that implements a mechanism, the issues where it failed, the release that changed it. Use Octocode tools when the host lists them; otherwise the same tools run from the `octocode` CLI (`octocode <tool> '{"queries":[…]}'`, fields in `octocode schema <tool> --view query`, or `npx -y octocode` when it is not installed), and `gh` covers issues, pull requests, releases and the API. Both need GitHub sign-in: `octocode auth status`, `gh auth status`. For a repository or package beyond one claim, the `octocode-research` skill owns the full tool routing.

| The claim turns on | Read on GitHub | Octocode tool | `gh` |
|---|---|---|---|
| Mechanism: how it works, what must be true | Source, docs, tests at a commit | `ghSearchCode` → `ghGetFileContent` with `ref` and `matchString` | `gh api repos/<owner>/<repo>/contents/<path>?ref=<sha>` |
| Failure cases and limits | Issues and pull requests: bugs, regressions, "migrating away" | `ghSearchHistory` (`issue`, `pullRequest`, `keywords`) → `ghGetHistoryItem` | `gh issue list -R <owner>/<repo> --search "<words>" --state all` |
| Alternatives and adoption | Repositories by topic; stars, `pushedAt`, `archived` | `ghSearchRepo` | `gh search repos "<words>"` |
| When a behavior changed | Commits, releases, a compare between versions | `ghSearchHistory` (`commit`), `ghStructure` (`refs`), `ghGetHistoryItem` (`compare`) | `gh release list -R <owner>/<repo>` |
| Who maintains it, its license | Repository facts, `LICENSE`, `CODEOWNERS`, governance files | `ghSearchRepo` (`get`), `ghGetFileContent` | `gh repo view <owner>/<repo>` |
| A package version or its trust | The source at the release tag, advisories | `artifactSearch` → `ghStructure` → `ghGetFileContent`; `securityCheck` | `gh api repos/<owner>/<repo>/security-advisories` |

- Cite a permalink pinned to a commit (`https://github.com/<owner>/<repo>/blob/<sha>/<path>`), an issue or a pull request URL; a `/blob/main/` link moves. The verifier reads blob, raw and issue pages, so copy the quote with `read-page.mjs` like any page.
- An issue is a report, not a confirmed defect, until a maintainer, a reproduction or a fix confirms it; record who wrote the quote.
- README result tables are often images the verifier cannot read; take the numbers from the paper or from a text file in the repository.
- Stars, forks and activity show attention, not quality or fitness.

## Combine and deepen

The web and GitHub answer different halves of a claim: the web holds studies, benchmarks, experience reports and comparisons; GitHub holds the artifact they describe. Each round of [step 7](../SKILL.md#steps) crosses them:

| A round found | Next round |
|---|---|
| A web claim about a project, version or benchmark | Read that repository: the code behind the mechanism, issues matching the failure, the release that changed it |
| A GitHub issue, regression or limit in code | A web query for its postmortem, benchmark or "migrating away from" story, and whether a release fixed it |
| A claim cited secondhand | The study or source it cites ([primary](verification.md#validated-sources)) |
| Support only | The rival's best case and the failure query, on both surfaces |

All rounds write to the same ledgers with the plan's claim ids, so `research-tree.mjs --verified` shows what each round changed. Stop a claim when a round changes neither its state nor its status, or it meets the [proof bar](proof-bar.md#contradict-and-prove); otherwise name the gap.
