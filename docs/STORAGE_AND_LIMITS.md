# Octocode home, storage and limits

The CLI and MCP use the same Octocode home and default to `storage.mode: "persistent"`. Change the mode to control what survives between calls. Run `octocode config get storage.mode` in the workspace to see the effective value. [Configuration](CONFIGURATION.md)

## Where everything lives

The home defaults to `~/.octocode` on macOS and Linux or `%USERPROFILE%\.octocode` on Windows. Set `OCTOCODE_HOME` to an absolute directory in each CLI, editor, or MCP environment to move it. Give each user a separate home on a shared host. Local tools can read checkouts and materialized outputs from that user's home.

| Location | What you will find |
|---|---|
| Home `.octocoderc` and `.env` | User-wide settings and trusted environment fallbacks. The project equivalents live in `.octocode/` under the process working directory. [Configuration precedence](CONFIGURATION.md#where-every-value-comes-from) |
| Home `credentials.json` and `.key` | Encrypted Octocode GitHub login and its encryption key. Use `octocode auth logout` to remove the stored login. [Authentication](AUTHENTICATION.md) |
| Home `skills/` | Skills installed by `octocode skill install` and linked into selected agent hosts |
| Home `clones/` and `clone-store/` | `ghCloneRepo` checkouts and the Git stores that back them. A clean checkout at the resolved commit can be reused after an access check. If it has local changes, Octocode refuses reuse and preserves the worktree. Keep its backing store while using a checkout. |
| Home `cache/provider-answers-v3/` | Reusable GitHub and package registry responses, separated by origin and credential |
| Home `tmp/` | Saved response pages, continuation handles, section assessments, LSP answers, graph query rows, GitHub rate-limit state, and clone staging |
| Home `tmp/materialize/v3/` and `tmp/capture/url/` | Commit-pinned GitHub files and captured URL or image evidence, respectively. Use the inspection path the tool returns. |
| Workspace `.octocode/graph/` | Published graph snapshots and temporary `.facts` checkpoints. Set `local.graphHome` to use `octocode-graph/` under a different directory. |

## Cache storage and lifecycle

| Behavior | `persistent` (default) | `memory` |
|---|---|---|
| Response pages, provider answers, and section plans | Saved under the configured home. An eligible cursor can resume from a later process. | Held in the current runtime. Paths and GitHub files can still be read directly. Use a running MCP server for multi-call section plans. |
| GitHub checkouts and materialized files | `ghCloneRepo` and `ghStructure` can write local outputs. | A request requiring materialization returns `persistentStorageDisabled`. An ordinary file read returns content without a materialized `location.path`. |
| URL and image captures | Inspectable captures are retained for 24 hours. A fresh URL request fetches again. | Capture requiring persistent storage is unavailable. |
| Explicit graph builds | Publish snapshots and checkpoint interrupted extraction. | Publish requested snapshots, with extraction intermediates held in RAM. |
| Maintenance | A runtime startup checks whether a daily cleanup is due. | Automatic cleanup and disk rate-limit mirrors are disabled. |

To use memory mode for one workspace, put this in its `.octocode/.octocoderc` and restart MCP. To select it for one shell invocation, prefix the command with `OCTOCODE_STORAGE_MODE=memory`. Existing files, installed language servers, and login credentials have their own lifecycle.

```json
{"storage": {"mode": "memory"}}
```

### What a repeated request does

| Evidence | Reuse and freshness |
|---|---|
| Local file, AST, or LSP | The runtime can reuse in-memory outlines, parsed declarations, and name mentions when the content matches. Cursor reads refer to the saved snapshot. |
| GitHub or registry response | An immutable SHA or an unexpired origin-supplied `max-age` can be reused directly. When an entry expires, a validator such as an ETag lets the origin confirm it. An unused answer expires after seven days. |
| GitHub search or rename | With no origin lifetime, search results and renamed repository routes may be reused for five minutes. `data.observedAt` identifies when a reused result was observed. A multi-section pull request can reuse the lifetime learned from a REST pull-request read for that credential. |
| GitHub file missing at a commit | A path missing at a verified immutable commit can be cached. A missing repository, ref, or path on a moving branch is checked again. |
| Sensitive response or advisory | Provider answers containing detected secrets stay in RAM. GitHub applies the configured path policy. OSV advisory answers are fetched fresh. [Security](SECURITY.md) |
| Classification | A fresh judgment can cost provider tokens even when the source is cached. Saved result pages replay completed work. [Classification and costs](OCTOCODE_CLASIFY.md#availability) |

### Clear cached work

Stop separate MCP processes, run the following command from the workspace whose graph checkpoints you want to clear, then restart those MCP processes.

```bash
octocode cache clear --json
```

The command drains warm CLI servers. Its receipt lists the directories removed from the configured home, including saved pages, provider answers, captures, and materialized files, plus this workspace's extraction checkpoints. Previously issued cursors may expire. Clones and their backing stores, published graphs, credentials, and installed servers survive a cache clear. [CLI cache command](OCTOCODE_CLI.md#clear-tool-caches)

In persistent mode, startup checks for expired entries at most once per 24 hours when the temporary directory exists. It removes aged materialized files under their snapshot locks, old captures, and expired saved work. Clone outputs have no automatic expiry.

## Fixed internal limits

| Resource | Default behavior |
|---|---|
| Response page | The automatic target is 20,000 characters. Set a positive integer `responseChars` on a fresh request to change the target. Follow `responsePagination.next` before tool-level continuations. Indivisible values may exceed the target. [Data contract](TOOL_DATA_CONTRACT.md#executable-continuations) |
| GitHub and registry HTTP | A connection or read can stall for 30 seconds. A flowing body can take longer. Retryable failures have up to three retries. |
| Git clone and fetch | Git ends a transfer below one byte per second for 30 seconds. A caller deadline or cancellation can also stop it. |
| Provider classification and URL capture | No default whole-call time budget. A silent connection can time out after 300 seconds without incoming bytes. |
| Classification concurrency | Up to 10 in-flight requests to a provider endpoint per process, shared across calls. Throttling reduces concurrency and a provider's `Retry-After` pauses new requests. |
| Classification page or section batch | About 131,000 characters of evidence and questions, with at least one unit admitted. The next batch covers the remaining units. |
| Retained work | The RAM budget is one eighth of known host memory, capped at 2 GiB; when host memory cannot be read, it defaults to 512 MiB. Each persistent store directory has a separate byte budget of the same size. |
| Language-server memory | The default per-server cap is one quarter of host RAM, bounded to 4,096–8,192 MiB. Override with the server's `maxMemoryMb`. |

A CLI or MCP tool call has no default wall-clock deadline. A caller can cancel it with CLI Ctrl-C or an MCP cancellation notification. For GitHub, a spent core or GraphQL quota returns `rateLimited` with its reset and retry route. Search quotas and secondary limits can wait until reset or `Retry-After`, subject to the caller's deadline. During long waits, terminal CLI clients get progress on stderr and MCP clients with a progress token receive notifications.

[Documentation hub](README.md)
