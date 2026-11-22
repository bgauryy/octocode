# Graph output for CLI and MCP

**Proposal.** Give an export two ways to reach its reader. Use `path` by default for agents, scripts and CI. Use `web` when a person wants to explore the result. Both should create the same saved graph and return a short receipt.

## Available today

| Task | Current interface |
| --- | --- |
| Build and query a native snapshot | The package CLI and MCP expose `graphBuild` and `graphQuery`. The snapshot ID pins subsequent queries. |
| Save a graph from a file or directory | `octocode-graph-build <source> --out <new-directory>` writes `data.json`, `events.jsonl`, briefs and chunk evidence. |
| Watch a build | `octocode-graph-serve <source>` opens the loopback viewer. `--no-open` prints its URL for headless use. |
| Make portable HTML from a saved run | `octocode-graph html <saved-run> <new.html>` embeds the saved graph, events, renderer and query worker. The HTML remains usable after the build process exits. |

## Proposed interface

| Input | CLI | MCP |
| --- | --- | --- |
| Source file or directory | `octocode-graph export src --output path --out ./review` | `graphExport({path: "src", output: "path"})` |
| Create and open a portable graph | `octocode-graph export src --output web --out ./review` | `graphExport({path: "src", output: "web"})` returns an HTML resource for the client to open. |
| Reuse a saved run | `octocode-graph export --from-run ./review --output web` | Supply `fromRun` in place of `path`. Building and LSP incur zero additional calls. |

Here `--out` always selects a new **directory**. The package owns the filenames. The receipt names the source, run directory, primary artifact, each artifact’s role and media type, chunk coverage, LSP state and elapsed time. For `path` the primary artifact is `data.json`. For `web` it is a standalone HTML file alongside the same data. An agent answering an ordinary graph question can still use a targeted, paginated `graphQuery`.

The local CLI opens portable HTML with the OS browser when asked for `web`. The `--no-open` option keeps the file for CI. If browser launch fails, preserve the successful export and report its path and launch error. Generating a web artifact on MCP returns a typed resource link and structured receipt. A remote client needs an artifact URI served by the MCP server, or a download its host can open. A server-local pathname or loopback URL alone reaches only the server machine. The MCP protocol supports resource links in tool results, and GitHub CLI provides precedent for explicit browser opening. [MCP tool results](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) · [MCP resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources) · [GitHub CLI](https://cli.github.com/manual/gh_repo_view)

## Live or portable

Portable HTML works offline, travels as one file, and leaves no server running. Its cost is waiting for the final artifact. The live viewer can show listing, chunk progress and the first stitched graph while LSP continues. Keep the existing serve operation for that workflow. If exposed through MCP later, a live run needs a progress handle, an explicit stop operation and a URL the client can actually reach.

## Acceptance checks

1. Build a directory and one file through CLI and MCP. Match the receipt to the canonical input and every artifact to an existing file or retrievable resource.
2. Convert one saved run to HTML. Compare decompressed embedded data and events byte for byte with their originals, confirm zero rebuilds, and run an offline query in Chrome.
3. Exercise desktop CLI, headless CLI and a client on another machine. Verify which machine opens a browser, how that client retrieves the artifact, and what remains available when launching fails.
4. Try a reused output directory, cancellation and an incomplete LSP phase. Preserve existing files, distinguish a stitched checkpoint from a finished export and retain partial evidence.
