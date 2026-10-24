Load for Octocode request examples. [Shared usage](octocode-usage.md) owns transport, availability and continuation rules. Discover the live schema before selecting a variant.

## Local examples

```bash
npx -y octocode localSearch '{"queries":[{"path":"src","matchString":["createCache","withCache"],"view":"files"}]}'
npx -y octocode localFetch '{"queries":[{"path":"src/cache.ts","ranges":[{"startLine":1,"endLine":60}]}]}'
npx -y octocode astSearch '{"queries":[{"operation":"match","path":"src","parser":"typescript","rule":{"pattern":"eval($X)"}}]}'
npx -y octocode lspSearch '{"queries":[{"operation":"references","path":"src/cache.ts","symbolName":"createCache","line":10,"includeDeclaration":false,"view":"files"}]}'
```

Replace illustrative paths, symbols and lines with observed evidence. Each string-array matcher has the same explicit regex/case semantics across supported tools. `workspaceSymbol` uses symbolQuery, not symbolName.

## Remote examples

```bash
npx -y octocode ghSearchRepo '{"queries":[{"operation":"search","keywords":["cache"],"filters":{"language":"TypeScript"}}]}'
npx -y octocode artifactSearch '{"queries":[{"operation":"resolve","ecosystem":"npm","packageName":"zod"}]}'
npx -y octocode ghCloneRepo '{"queries":[{"owner":"bgauryy","repo":"octocode"}]}'
```

Clone is CLI-only and requires persistent storage. Use a discovered repository and revision for real work; run local tools at the returned path. Partial materialization limits local coverage.

## Verification

Follow emitted input unchanged. Public schemas contain authored selectors; private offsets and snapshots are encapsulated by cursor handles. Check actual CLI/MCP behavior after rebuilding a changed contract. Run the dev skill for monorepo checks and the [workflow guide](workflows.md) for research/review procedure.
