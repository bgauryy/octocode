# oqlSearch (`octocode search`)

Unified search CLI that routes one query to the right tool. **Env-gated** — run
with `ENABLE_OQL=1`. Full reference: `search --scheme`. Snippets are discovery,
not proof — decide from exact content, PR/commit metadata, or LSP/graph proof.

```bash
CLI="ENABLE_OQL=1 node packages/octocode/out/octocode.js"
```

## Shape (`search --scheme`)

- **SOURCE** (where): `local` (`./src`) · `github` (`owner/repo`) · `npm` (`--target packages`) · `materialized` (a prior clone/cache localPath).
- **TARGET** (`--target`, or inferred): `code`, `content`, `structure`, `files`, `semantics`, `repositories`, `packages`, `pullRequests`, `commits`, `diff`, `research`/`graph`, `materialize`.
- **LSP semantics** (run `documentSymbols` first): `references`, `callers`, `hover`.
- Routing debug: `search --explain --query '{...}'`.

## Checks

1. **Local code** — `$CLI search "buildDirectToolCommandPatterns" ./packages/octocode-tools-core/src`
   → PASS: routes to local code search; file+line anchors.
2. **GitHub code** — `$CLI search "localSearchCode" bgauryy/octocode` → PASS: routes to GitHub; owner/repo preserved.
3. **Content view** — `$CLI search bgauryy/octocode/README.md --content-view none` → PASS: reads the file verbatim.
4. **Packages** — `$CLI search zod --target packages` → PASS: routes to npm; resolves source repo.
5. **Structure** — `$CLI search ./packages --target structure` → PASS: dir layout.
6. **Files** — `$CLI search --target files ...` by name/glob/ext → PASS: file finder.
7. **Semantics** — `$CLI search ./packages/octocode-tools-core/src/tools/toolNames.ts --op documentSymbols` then `--op references --symbol isLocalTool --line <L>` → PASS: LSP defs/refs.
8. **PRs / commits** — `--target pullRequests` / `--target commits` on a repo → PASS: routes to history research.
9. **Routing parity** — `$CLI search --explain --query '{...}'` → PASS: `--explain` names the same backend route the run used (shorthand, `--query`, and `--dry-run` agree).
10. **Empty honesty** — GitHub zero rows → PASS: states "NOT absence", suggests structure/clone/materialize proof.

## Workflows

- **One entry, right tool**: prove shorthand and `--query` reach the same tool as the raw `tools <name>` call (parity with the per-tool checks).
- **Materialize for AST/LSP**: `--materialize required` turns a remote repo local so structural/semantic ops work.
- **Dead-code sweep**: `--target research`/`graph` for reachability.
