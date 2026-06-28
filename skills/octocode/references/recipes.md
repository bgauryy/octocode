# Octocode CLI — Worked Recipes

Read this when you need concrete multi-step command sequences. Each shows the cheapest path; adapt paths and `owner/repo` to your target.

## Symbol lookup (definition + callers)

```bash
npx octocode search 'runCLI' bgauryy/octocode-mcp --lang ts --limit 10
npx octocode search bgauryy/octocode-mcp/packages/octocode/src/cli/index.ts --match-string 'export function runCLI' --content-view exact
```

## Workspace mapping (layout + each package.json)

One `search --tree --depth 2`, then parallel `search <package.json> --content-view exact` calls for each `package.json` in a single message.

## Deep multi-file work in one repo (>~3 files)

`npx octocode clone owner/repo`, then run `npx octocode search`, `npx octocode search --pattern` (AST), `npx octocode search --symbols`, `npx octocode search --op`, and `npx octocode search --content-view exact` on the local clone instead of many GitHub round-trips.
