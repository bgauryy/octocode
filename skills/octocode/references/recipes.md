# Octocode CLI — Worked Recipes

Read this when you need concrete multi-step command sequences. Each shows the cheapest path; adapt paths and `owner/repo` to your target.

## Symbol lookup (definition + callers)

```bash
octocode search 'runCLI' bgauryy/octocode-mcp --lang ts --limit 10
octocode search bgauryy/octocode-mcp/packages/octocode/src/cli/index.ts --match-string 'export function runCLI' --content-view exact
```

## Workspace mapping (layout + each package.json)

One `search --tree --depth 2`, then parallel `search <package.json> --content-view exact` calls for each `package.json` in a single message.

## Deep multi-file work in one repo (>~3 files)

`octocode clone owner/repo`, then run `search`, `search --pattern` (AST), `search --symbols`, `search --op`, and `search --content-view exact` on the local clone instead of many GitHub round-trips.
