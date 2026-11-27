Load when evidence moves between local code, packages and GitHub, or a route has a condition: versions, CVEs, refs, fetched code. Other parts: [routes](tool-flow.md), [shape and scout](tool-flow-shape.md), [analysis, history and recovery](tool-flow-recover.md).

## Carry evidence across

| You have | Send it as | To | You get |
|---|---|---|---|
| Import in local code | symbol + hit line | `lspSearch` definition | The installed file in `node_modules` |
| Lockfile or installed `package.json` | `packageName`, `version` | `artifactSearch` resolve | `owner`, `repo`, `ref`, `verification` |
| Lockfile or installed `package.json` | `ecosystem`, `packageName`, `version` | `securityCheck` package | `affected`, `upgradeTo` and CVE ids at that version (the `answer` sentence lists up to five; every advisory's `cves` is in the details, `advisories[].cves`) |
| Resolved `ref` | `ref` | `ghStructure`, then `ghGetFileContent` | Source of that exact release |
| Two resolved versions | `baseRef`, `headRef` | `ghGetHistoryItem` compare | What changed between releases |
| Commit SHA + paths | `ref`, `include`, `materialize:true` | `ghStructure` | `location.path` for local tools |
| Local path + git remote | `owner`, `repo`, `path` | `ghSearchHistory` commit | Commits that touched it, then their PRs |
| A line in local code | `<start>,<end>:<path>`; else `path` | `git log -L` when a shell is available; else `ghSearchHistory` commit, then `ghGetHistoryItem` patches with `include` | The commit, then its PR |
| History `files` row | path + commit as `ref` | `ghGetFileContent` | The code at that revision |
| Local or GitHub path | `source` | `clasify` | A screen of unread files, then `next.read` |

## Route conditions

- **Version in use beats latest.** Omitting `version` resolves the latest stable release, so pass the lockfile's version and read installed files first.
- **A CVE question goes to `securityCheck` when it is listed:** `package` at the lockfile's version, `url`, `advisory` with an id, or `commit`. `affected:false` with `advisoryCount:0` is none known to OSV, not proof the version is safe; `status:"error"` is not checked. When it is not listed (MCP without `OCTOCODE_ENABLE_SECURITY`; the CLI always offers it), memory is not evidence: name the gap. `artifactSearch` carries `advisories` only for PyPI and NuGet, from the registry, and elsewhere the state is unknown, never clean. With a CVE already named, prove the fix from source: the release's `ref`, then `ghSearchHistory` and `ghGetHistoryItem` compare.
- **Trust the ref by `verification`.** `provenance` and `tag` tie it to the version; with `registryRef` or `defaultBranch`, check that tree's manifest before citing. PyPI resolves only; custom registries are npm only.
- **`ghSearchCode` sees only the default branch.** Its `ref` re-reads hits without widening the search. For another revision, use `ghStructure` at that ref; paths may differ. An empty index result is not absence.
- **`ghSearchCode` requires `owner`.** With no owner, start from `artifactSearch` or `ghSearchRepo`; never guess.
- **Match search scope to the claim.** LSP references can reach workspace importers beyond the anchor package; graph, AST and text stay within their requested paths.
- **AST and graph edges are candidates.** Check grammar capabilities, parse assessment and scan coverage, then confirm with LSP. Empty LSP while `analysisContext` reports errors is not absence.
- **Graph needs the workspace.** When listed, `graphBuild` rejects a materialized or cloned path outside it; use AST and text there.
- **Materialize until `location.coverage.state` is `complete`.** A large selection returns `partial`; follow `next.continueMaterialize`.
- **Fetched code has no LSP by default.** Text and AST work on materialized and cloned trees. `lspSearch` refuses them (`errorCode:"policy"`), and any root holding them, because a server can run repository code; only the user's `lsp.trustFetchedCode` opts in. With it on, LSP reports `inferredProject` without a tsconfig and `documentAnalysisErrors` when imported files were left out, so include project config and the source tree, then read `coverage`. A whole-repo claim needs `ghCloneRepo` (CLI, or MCP with `OCTOCODE_ENABLE_CLONE` and the local tools on); sparse paths limit what it proves.
- **Discussion lives only on GitHub.** Prefer a local checkout for code; use `ghGetHistoryItem` for PR intent.
