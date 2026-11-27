# Code graph

Find what calls what, what a change affects, how two files connect, cycles, hotspots and critical code, and what a
change did to the structure. The graph comes from the Octocode engine; answers cite `path:line`.

## Turn it on

Install `@octocodeai/octocode-graph` for the `octocode-graph` CLI, or add one stdio MCP server, `octocode-graph mcp`:
saved graphs (`build_graph`, `query_graph`, `refresh_graph`, `diff_graph`, `link_graph`) and the engine's `graphBuild`
and `graphQuery`. `octocode-graph channel` adds the live viewer for Claude Code.
Engine snapshots live under the graph workspace's `.octocode/graph/` by default
([change the location](CONFIGURATION.md#all-settings-and-defaults)); saved graphs go under
`.octocode/octocode-graph/`.

## Build once, ask many times

```bash
octocode-graph build .                                   # a stitched graph of every package, calls confirmed by LSP
octocode-graph query . "search parseArgs"                 # find a name you partly know
octocode-graph query . "about src/config.ts"              # metrics, community, dependencies and dependents
octocode-graph query . "impact src/config.ts depth 2"     # what a change may affect
octocode-graph refresh .octocode/octocode-graph/<run>     # after edits: only changed packages rebuild
octocode-graph diff .octocode/octocode-graph/<run>        # what that refresh changed in the structure
```

`query .` answers from the newest saved graph of the directory and warns when files changed since. Several
questions answer from one load with `--stdin`. Queries may be written naturally (`callers of x`, `top pr`, a typo):
the answer says how it was read.

| Question | Query |
|---|---|
| Who calls this function? | `callers <name>`, then the `confirm` lspSearch call it returns for a complete list |
| What depends on this file, or what does it use? | `dependents <file>`, `deps <file>`, `impact <file>` |
| How does A reach B? Does the CLI bypass the service layer? | `path <a> <b>`, `path <a> <b> --avoid <dir>` |
| Where are the cycles, and what moves together? | `cycle <file>`, `order <file>` |
| What is critical? | `critical functions`, `critical points`, `critical path <file> --up`, `top pagerank` |
| What can take part in this flow? | `region <entry> <outcome>` |
| What did a change do? | `octocode-graph diff <before> <after>` |
| Which docs and configs name this file, and what do they name that no longer exists? | `about <file>` (documented by), `links --unresolved`, `links --stale` |

## Docs, configs and agent links

A build reads the Markdown files and manifests under its root: doc links, backticked paths, `path:line` citations,
placed symbol names, and the files `package.json`, `Cargo.toml` and `tsconfig` fields name become links with their
line. What names no single target is kept with its reason and candidates. An agent settles one with
`octocode-graph link <run> add '…'`: the link is stored only when its quotes are at their lines, and every build
checks them again. [Links](../packages/octocode-graph/skills/octocode-graph/references/links.md)

## One scope, from the engine

For a single directory, or the engine's own detectors (unused exports, boundary violations, cochange, drift):

```bash
octocode-graph graphBuild '{"queries":[{"path":"src"}]}'
octocode-graph graphQuery '{"queries":[{"operation":"issues","path":"src"}]}'
```

Use the build's `path` in every query, or pass its `graph` value to pin that snapshot. Run
`octocode-graph schema graphQuery --view variants` for every operation.

## Read the answer

Edges are syntax evidence, confirmed by the language server where it was asked: before a claim, open the cited
lines (`localFetch`) and check an uncertain binding with `lspSearch`. Impact lists what may be affected; tests or
execution establish breakage. [Graph skill](../packages/octocode-graph/skills/octocode-graph/SKILL.md) ·
[Local code tools](OCTOCODE_TOOLS.md#local-code-tools-reference)
