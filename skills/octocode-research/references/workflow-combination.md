# Local and external evidence

Load when a local clue points upstream or remote source needs local AST, LSP, graph, or multi-file evidence. This reference owns the materialization decision.

## Local to external
- Resolve the local package/version or exact error/config anchor before searching upstream.
- For a repository concept without a known owner/name, start with `ghSearch operation:"repositories"`; keep alternative concepts in separate queries.
- Use npm metadata to locate the repository and package subdirectory; match a release/tag/commit before comparing behavior.
- For a known commit, read it directly. For history discovery, search the relevant repository/path or message, then fetch the chosen commit/PR.
- Return to local callers, configuration, and tests before claiming an upstream change fixes the running system.

## External to local
Choose the smallest scope that supplies the required evidence:
| Need | Tool | Scope and caveat |
|---|---|---|
| One remote read | `ghGetFileContent` | exact file/ref; no materialization needed |
| Directory inspection | `ghGetFileContent type:"directory"` | inspect returned completeness and skipped/partial state |
| Repeated reads of a known subtree/file | `ghCloneRepo` with optional `sparsePath` | checkout may include root files; complete is relative to the requested scope |
| Repository-wide graph or semantic project | `ghCloneRepo` without `sparsePath` | shallow checkout; shallow history is not full history |

Use the returned `location.localPath` for clone results, or the directory result's returned local path; never synthesize cache paths. Preserve requested/resolved ref, `commitSha`, and scope. Clone `branch` accepts a branch, tag, or full 40-character commit SHA. For reproducible evidence, select an immutable SHA and retain the returned identity.

Availability depends on the live catalog, `ENABLE_LOCAL`, `ENABLE_CLONE`, and `OCTOCODE_STORAGE_MODE`. Clone and directory materialization require persistent storage. Declare a disabled capability; use remote evidence or an existing checkout without changing global configuration automatically.

## Scope is part of proof
- Choose materialization based on needed evidence and cost, not a read-count threshold.
- A sparse checkout can answer a scoped syntax question while omitting imports, tests, manifests, or LSP configuration needed for semantic proof. Expand scope when those dependencies matter.
- Inspect `cached`, `complete`, and `verified` separately: a cached checkout can retain its scope while its working files have changed. `commitSha` identifies HEAD, not every working file's bytes. Use `forceRefresh` when fresh verification matters.
- Cloning source and executing source are separate actions. Carry existing user authorization; follow applicable execution restrictions and inspect scripts before running untrusted code.
- Feed the returned path into the local route, inspect diagnostics, and keep graph edges syntactic until corroborated. Local tools cannot prove absence outside the materialized scope.

Next: run `references/workflow-local.md` on the materialized path; use `references/workflow-external.md` for remote evidence; apply the authorization rule in `SKILL.md` when scope changes.
