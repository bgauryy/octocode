Load when choosing a command or its flags: preflight, serve, build, query, diff, refresh, check, cache, render, events, html.

# Commands

Requires Node 24.15+ and a browser for the viewer; `rsync` supports the frozen-copy fallback. The viewer serves its bundled renderer locally. `octocode-graph preflight` reports available layers.

Every command below also runs in the npx form [SKILL.md](../SKILL.md) gives. The CLI's own hints, usage lines and `brief.txt` name `octocode-graph` only when the first `octocode-graph` on the shell's PATH runs this package (a package runner's `node_modules/.bin` does not count), and the npx form otherwise.

Pass a file to build or serve a graph of that file alone. Pass its enclosing directory when you need cross-file imports and calls. Run from the target workspace, which defines paths and allowed roots. Use the command’s `--help` for options. The graph package gets its engine from its `octocode` dependency. Set `OCTOCODE_CLI` to select a specific engine. The build and viewer validate options before starting work. In a channel with multiple viewers, each result directory records the tool calls for its own build and refreshes. Use a new output directory for each build. The selected CLI’s full catalog identifies the executable and each tool’s batch limit. The pipeline freezes that executable in a private directory under the OS temp directory, keyed by its SHA-256 digest. Runs can reuse the same image and warm language servers. An image no run used for a day is removed, unless a running process pinned it. An incompatible schema stops the build before file listing.

| Goal | Command |
|---|---|
| What runs here | `octocode-graph preflight [--json]` |
| Live viewer | `octocode-graph serve <section> [--out <dir>] [--whole] [--lsp smart\|all\|off] [--jobs N] [--ingest-jobs N] [--exclude a,b] [--port N] [--no-open] [--exit-when-done]` |
| Headless result | `octocode-graph build <section> [same flags] [--progress] [--no-cache] [--explain-cache]`. A section inside a workspace (an ancestor with package.json `workspaces`, `pnpm-workspace.yaml`, a Cargo `[workspace]` or `go.work`) whose imports name packages of that workspace outside the section leaves them as externals: stderr, `brief.txt` (`SCOPE`) and `data.json` `workspaceExternals` in data.json `coverage` name each package with its member directory and import count, and the build of the workspace root that stitches them. A section inside one member package is that package's own: its imports into the rest of the package come first (`scope: "package"`) with the build of the package root, the smaller build |
| Ask a saved graph | `octocode-graph query <result-dir\|data.json\|project-dir> "<query>" [--json] [--stdin] [--refresh]`. A project directory answers from its newest graph under `.octocode/octocode-graph` (of the current directory or the project), else the newest the graph registry lists for that root. `--refresh`: when graph files changed since the graph was written, the chunks holding them refresh first (as `refresh`) and the answer comes from the refreshed graph; unchanged, it answers directly. Exit 0 answered, 1 a query error or a broken rule (`failed: true`, `reach --forbid`), 2 a usage error or no saved graph |
| What changed structurally | `octocode-graph diff <result-dir>` (the graph the last refresh replaced, kept as `previous.json`, against `data.json`) or `octocode-graph diff <before> <after> [--json]`. [Diff snapshots](diff.md) |
| Update a saved result after edits | `octocode-graph refresh <result-dir\|project-dir> [--lsp smart\|all\|off] [--progress] [--no-cache]`: rebuilds only the chunks holding a change (an added or removed package manifest rebuilds the whole root), then stitches, checks and measures again. The LSP pass probes only functions an edit could change: one whose file changed, or one that calls into a changed file; the rest keep their previous answers (`callStats.carried`). Events append to `events.jsonl`. A run a live viewer serves refreshes through that viewer, so its page updates |
| Dependency rules for CI | `octocode-graph check [<result-dir\|project-dir>] [--rules <file>] [--format text\|json\|sarif] [--baseline <file>] [--write-baseline] [--since <rev>]`: exit 1 on a new error violation, 2 when the rules or arguments cannot be used. [Rules](rules.md) · [CI use](rules-ci.md) |
| Reopen a finished run | `octocode-graph serve --replay <result-dir>` |
| A standalone page of a finished run | `octocode-graph html <result-dir> <new.html>` ([delivery](../output.md#viewer-delivery)) |
| The shared graph cache | `octocode-graph cache stats` (entries and bytes by kind, JSON) or `octocode-graph cache prune --max-bytes N` (evicts the least recently used until the entries hold at most N bytes) |
| Briefs again (after `model.json`) | `octocode-graph render --data <dir>/data.json` |
| Tool calls, errors and time of a run | `octocode-graph events <result-dir\|project-dir> [--json]`: summarizes `events.jsonl` ([events](viewer-events.md#viewer-and-server)) |

Use `--lsp smart` (default) for callable declarations in hub files and callers crossing a chunk boundary. Use `--lsp all` to probe every indexed function, method and constructor with a declaration line, including those with no static calls. `off` skips confirmation. The saved probe totals and unanswered reasons describe coverage.

Build phases, the graph registry and the shared cache: [build pipeline](build-pipeline.md). Server binding and tokens, reconnects and `events.jsonl` event types: [viewer and events](viewer-events.md).
