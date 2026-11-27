# Storage and caches

Run `octocode config home` to find your Octocode home and `octocode config get storage.mode` to see how results are stored.

## Where files live

The default home is `~/.octocode` on macOS and Linux or `%USERPROFILE%\.octocode` on Windows. Set `OCTOCODE_HOME` to an absolute path to use another location.

| What you want to find | Where to look |
|---|---|
| User settings | `.octocoderc` and `.env` in the Octocode home. Project settings come from `.octocode/` under the launch directory. [Settings precedence](CONFIGURATION.md#where-every-value-comes-from) |
| Saved GitHub login | `credentials.json` and its companion key in the home. Use `octocode auth logout` to clear the saved login. |
| A cloned or downloaded file | Use `ghCloneRepo`’s `location.path` for a checkout or the materialized paths returned by `ghStructure`. |
| Code graphs | `.octocode/graph/` in the workspace, or the directory configured with `local.graphHome`. |

<a id="materialize-git-store"></a>

## Git checkouts and history

`ghCloneRepo` resolves the requested ref with your credential on every call. One bare Git store per host and repository holds the commit history. The first fetch imports branch heads and tags with `--filter=blob:none`. A checkout downloads the file content its scope needs. A pin missing from the store triggers a fetch that refreshes the remote refs. Each checkout stays detached at the commit in its receipt.

Use `git log HEAD` in the returned path to walk the pinned history. Branches from the last fetch are under `refs/remotes/origin/` and remote tags under `refs/octocode/tags/`. Local branches and tags remain user-owned. Every checkout holds the commit's full history from the shared store, so one commit and one `paths` scope share one checkout path. Paths returned by earlier releases, including checkouts made with the removed `historyDepth`, remain usable.

## Cache storage and lifecycle

Persistent storage lets another CLI call resume a saved page and lets GitHub tools save checkouts and selected files. Memory mode keeps cached results in the running process. Keep the same MCP session open for an in-memory continuation. Requested graph snapshots still write to disk.

To use memory mode for a project, put this in `.octocode/.octocoderc` and restart MCP.

```json
{"storage":{"mode":"memory"}}
```

### Clear cached work

Stop any separate MCP processes first. Then run this from the workspace whose graph checkpoints you want to clear, and restart MCP:

```bash
octocode cache clear --json
```

The command removes saved pages, provider answers, captures and materialized files in the home, plus this workspace's graph checkpoints. Cursors issued before the clear may expire. Clones, their Git stores, published graphs, credentials and installed language servers survive. [CLI cache command](OCTOCODE_CLI.md#clear-tool-caches)

[Configuration](CONFIGURATION.md) · [Read paged results](TOOL_DATA_CONTRACT.md#executable-continuations)
