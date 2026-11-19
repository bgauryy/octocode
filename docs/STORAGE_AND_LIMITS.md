# Storage and limits

The CLI and MCP server share one Octocode home. Its default storage mode is `persistent`, which keeps results between calls. Use `memory` to keep nothing on disk.

```bash
octocode config get storage.mode
```

## Where files live

The home is `~/.octocode` on macOS and Linux, or `%USERPROFILE%\.octocode` on Windows. Set `OCTOCODE_HOME` to an absolute path to move it. On a shared host, give each user a separate home.

| Path | Contents |
|---|---|
| `~/.octocode/.octocoderc` and `.env` | User-wide settings and environment values. A project's equivalents live in `.octocode/` in the working directory. [Where every value comes from](CONFIGURATION.md#where-every-value-comes-from) |
| `credentials.json` and `.key` | Your encrypted GitHub sign-in and its key. `octocode auth logout` removes them. [Authentication](AUTHENTICATION.md) |
| `skills/` | Skills from `octocode skill install` |
| `clones/` and `clone-store/` | Checkouts from `ghCloneRepo` and the Git stores behind them. Keep the store while you use a checkout |
| `cache/provider-answers-v3/` | Reusable GitHub and registry responses, kept apart by origin and credential |
| `tmp/` | Saved response pages and continuations, section assessments, LSP answers and graph query rows |
| `tmp/materialize-git/` | Temporary Git stores for one `ghStructure` materialize call |
| `.octocode/graph/` in the workspace | Published graph snapshots. Set `local.graphHome` to use another directory |

Captured URL and image evidence and materialized GitHub files are written under the home. Use the path the tool returns.

## Materialize git store

A `ghStructure` materialize call that reads bytes through Git uses a temporary store under `tmp/materialize-git/`. It removes the store when the call finishes, fails or is cancelled. If the process is killed, the next materialize call removes stores whose process is no longer running. [How materialize reads bytes](OCTOCODE_TOOLS.md#ghstructure)

## Cache storage and lifecycle

| Behavior | `persistent` (default) | `memory` |
|---|---|---|
| Response pages, provider answers and section plans | Saved under the home. A later process can resume a cursor | Held in the running process. Multi-call section plans need one running MCP server |
| GitHub checkouts and materialized files | `ghCloneRepo` and `ghStructure` write local outputs | Materialize requests fail with `persistentStorageDisabled`. A plain file read still returns content, without a `location.path` |
| URL and image captures | Kept for 24 hours. A new URL request fetches again | Unavailable |
| Graph builds | Publish snapshots and keep each file's extraction for the next build | Publish requested snapshots. Intermediate data stays in RAM |

To use memory mode for one workspace, put this in `.octocode/.octocoderc` and restart MCP:

```json
{"storage": {"mode": "memory"}}
```

To use it for one command, prefix it with `OCTOCODE_STORAGE_MODE=memory`.

## How repeated requests are reused

| Evidence | Reuse |
|---|---|
| Local file, syntax tree or LSP answer | Reused while the file content is unchanged |
| GitHub or registry response | An immutable SHA, or an unexpired `max-age` from the origin, is reused directly. Expired entries are checked with the origin's validator, such as an ETag |
| Unused saved answer | Expires after 7 days without use |
| Sensitive response | Answers with detected secrets stay in RAM only |
| Classification | Each new judgment costs provider tokens, even when the source is cached. Saved result pages replay finished work. [Classification and costs](OCTOCODE_CLASIFY.md#availability) |

`data.observedAt` in a reused result shows when it was first observed.

### Clear cached work

Stop any separate MCP processes first. Then run this from the workspace whose graph checkpoints you want to clear, and restart MCP:

```bash
octocode cache clear --json
```

The command removes saved pages, provider answers, captures and materialized files in the home, plus this workspace's graph checkpoints. Cursors issued before the clear may expire. Clones, their Git stores, published graphs, credentials and installed language servers survive. [CLI cache command](OCTOCODE_CLI.md#clear-tool-caches)

## Fixed internal limits

| Limit | Value |
|---|---|
| Response page | 20,000 characters by default. Set a positive integer `responseChars` on a new request to change it. Follow `responsePagination.next` before any tool-level continuation. A single value larger than the target can exceed it. [Continuations](TOOL_DATA_CONTRACT.md#executable-continuations) |
| Network stall | A GitHub or registry request, or a Git transfer, ends after 30 seconds without progress. Git also ends a transfer that stays below one byte per second for 30 seconds |
| Retries | Up to 3 retries for retryable GitHub and registry failures. Retry-After values are honored |
| Classification concurrency | Up to 10 requests in flight per process, shared across calls. Throttling lowers this. A provider's `Retry-After` pauses new requests |
| Classification batch | About 131,000 characters of evidence and questions per batch. Each batch admits at least one unit. The next batch covers the rest |
| Cancellation | Ctrl-C in the CLI, or an MCP cancellation notification, stops a call |
| GitHub rate limit | When the core or GraphQL quota is spent, the tool returns `rateLimited` with the reset time and a retry route |

Long waits print progress on stderr in the CLI. MCP clients that send a progress token get progress notifications.
