# Octocode home, storage and limits

What Octocode keeps on disk, how long, and which limits are fixed. To change a setting, see [Configuration](CONFIGURATION.md).

## Where everything lives

The **Octocode home** is one folder shared by the CLI, the MCP server, the VS Code extension, and installed skills: `~/.octocode` on macOS and Linux, `%USERPROFILE%\.octocode` on Windows. `XDG_CONFIG_HOME` and `APPDATA` are not used. To move it, set `OCTOCODE_HOME=/custom/path` for every product. Use [cache clearing](OCTOCODE_CLI.md#clear-tool-caches) to remove retained work while preserving checkouts. Current clone outputs live in `clones/`; older versions stored them under `tmp/clone/`. Deleting `credentials.json` and `.key` signs you out.

| Path in the home | Owner | What it holds |
|---|---|---|
| `.octocoderc` | you | Global Octocode settings. See [`.octocoderc`](CONFIGURATION.md#set-a-value). |
| `.env` | you | Trusted environment fallbacks: tokens, the classification key, third-party keys. Only this file may set [home-only keys](CONFIGURATION.md#where-every-value-comes-from). |
| `credentials.json`, `.key` | Octocode | The encrypted GitHub login and its key. `auth logout` removes both. |
| `.credentials.lock` | Octocode | Serializes credential writes between processes. |
| `skills/` | Octocode | Skills installed with `npx octocode skill install`; hosts such as Claude, Cursor, or Codex link to these copies. |
| `clones/` | output | `ghCloneRepo` checkouts, keyed by owner, repo, commit, sparse selection, history depth, and GitHub host. A clean checkout at the resolved commit is reused; one with local edits is refused and kept. |
| `tmp/response-pages/`, `tmp/continuations/`, `tmp/section-assessments/`, `tmp/lsp-answers/`, `tmp/graph-rows/` | retained work | Saved pages, issued continuations, section assessments, the LSP provider evidence selected by a continuation, and the complete ordered rows of a graphQuery `issues`, `impact` or `drift` answer, so its later pages slice them instead of computing them again. Clearing them invalidates their handles; a graph page whose rows are gone computes them again. |
| `tmp/capture/url/` | output | Freshly captured URL text and images exposed through inspection paths, retained for 24 hours. Fresh URL requests fetch again. |
| `tmp/materialize/v3/` | output | Files materialized at an immutable commit SHA for local inspection. Fresh materialization reacquires bytes; its manifest records published files. |
| `tmp/ratelimit/` | coordination | Last-known GitHub rate-limit state, shared across processes. |
| `tmp/clone-locks/`, `tmp/clone-tmp/`, `tmp/git-home/` | working state | Per-repository clone locks, staging, and an isolated git home. |

One home holds one user's retained work and outputs. Every clone call checks the caller's GitHub access and resolves the ref to a commit; it fetches that commit unless a checkout of it is already verified (HEAD is the commit, no local changes), which it reuses; local tools can read checkouts under `clones/`. On a host that serves several users or tokens, give each tenant its own `OCTOCODE_HOME`.

Clone git runs without your git config. It keeps `PATH`, the proxy variables (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`, either case), and CA bundles (`SSL_CERT_FILE`, `SSL_CERT_DIR`, `GIT_SSL_CAINFO`, `GIT_SSL_CAPATH`, `CURL_CA_BUNDLE`) from the runtime environment; a project `.env` cannot set them.

Native HTTP clients also honor `SSL_CERT_FILE` and `SSL_CERT_DIR` from the process environment. PEM certificates supplement the operating system trust store; unreadable, empty or malformed configured stores fail client construction. Certificate verification stays enabled. `SSL_CERT_DIR` uses the platform path-list separator.

A skill without a project writes reports and state to a named home folder, for example `research/` or `agents-communication/`. Inside a project, skills use `<project>/.octocode/`, which also holds the workspace `.octocoderc` and `.env` and `graph/` (the stored code graph).

## Cache storage and lifecycle

Fresh local reads, AST, LSP and classification requests recompute their answers. GitHub and registry answers are kept under `<OCTOCODE_HOME>/cache/provider-answers-v1`, one credential and origin apart, and the origin decides what a repeat costs: a read named by a commit, blob or tree SHA sends nothing; a `Cache-Control: max-age` answer sends nothing until it lapses; past that, an `ETag` or `Last-Modified` validator asks, and GitHub's 304 is free of its rate limit. GitHub search offers neither, so its answer stands for 5 minutes. Any answer reused without asking, or served because the origin rate-limited the request, carries a hint with the time it was observed. An answer any read would redact (one holding a credential) stays in memory only, and one unused for 7 days is dropped. CLI and MCP also retain the exact evidence and progress behind issued continuations under `OCTOCODE_HOME`; there is no database or editor-local storage.

`storage.mode: "persistent"` is the default. Set `storage.mode: "memory"` in `<OCTOCODE_HOME>/.octocoderc` (all workspaces) or `<workspace>/.octocode/.octocoderc` (one workspace), or set `OCTOCODE_STORAGE_MODE=memory`, to keep continuation state and provider answers in RAM only. Each runtime uses its own resolved policy and home. Persistent state is read from disk without a duplicate memory copy; memory state lasts only within its runtime.

Memory mode disables clone, directory and exact-file materialization, persisted continuation reads and writes, automatic maintenance, rate-limit mirrors and stats writes. File reads still return content without a materialized `localPath`. Explicit graph builds still publish their requested snapshots under the workspace's `.octocode/graph/`, but keep extraction intermediates in memory and never save `.facts` checkpoints. The setting does not delete existing files or credentials; managed language-server installations, compiled programs, HTTP connections and live language-server protocol state are separate operational resources.

| Concern | Behavior |
|---|---|
| Cleanup | At most one sweep per 24 hours (`tmp/.last-cache-maintenance`). Aged materialized snapshots are removed under their own locks; old tree staging, rate-limit mirrors and URL captures are swept by age. Entry stores enforce their own TTL and byte budgets. Clone outputs are never expired or evicted. |
| When | Synchronously at native runtime start: once per CLI process and at MCP server start. No background timer. Skipped in `memory` mode or when `tmp/` does not exist. |
| Failure | Best-effort; an unavailable or read-only state home never blocks the CLI or MCP startup. |
| Ownership | Only Octocode-owned directories; other `tmp/` content stays. |
| Manual | Stop separate MCP processes, then run `npx octocode cache clear` from the workspace. It drains warm CLI servers and clears the configured home's retained work plus this workspace's graph extraction checkpoints. See [cache clearing](OCTOCODE_CLI.md#clear-tool-caches). |

Published graph snapshots, materialized files and captures are inspectable outputs. Graph extraction checkpoints support interrupted jobs and are discarded after successful publication. Response pages and section assessments keep settled evidence so pagination does not repeat upstream requests or inference; issuing a new request performs fresh work. Credential selection also reloads its source on every request.

Retained-work limits are derived from one budget, not fixed numbers: memory state shares the process memory budget (an eighth of the memory available to the process, within 512 MiB to 2 GiB; a container's cgroup limit counts), and each store directory's disk budget equals that same value, which is also the largest single entry. They are per store, not a process-memory limit. Disk admission serializes writers and reserves space before publication. Entry headers and bodies count toward the budget; filesystem allocation, bookkeeping and one temporary atomic-write copy do not. Failed writes are reported to the continuation owner, which must return complete evidence or an explicit refusal instead of issuing an unreachable handle. Existing retired result-cache directories can be removed with `cache clear`.

## Fixed internal limits

These are code constants, not settings (one owner: `RuntimeLimits` in `packages/octocode-native/crates/runtime/src/execution/limits.rs`). They have no environment or settings-file overrides.

| Limit | Value |
|---|---|
| Network request timeout | 30 s |
| Retries after a failed request | 3 |
| Automatic response page | 20,000 characters (`responseChars` per call, up to 50,000) |
| URL captures | 24 h retention |
| Classification requests in flight per endpoint | 10 |
| Context selection per operation | 64 candidates, 131,072 characters, 10 s |
| Language-server memory | 25% of host RAM, between 4096 and 8192 MiB (per-server `maxMemoryMb` overrides) |

Saved-page sizes and the tool-call budget (300 s, one for every surface: CLI, warm server and MCP; an embedder may set its own) are fixed too.

- **GitHub rate limits:** a call waits out a primary or secondary limit or search pacing, and fails at once with `rateLimited` and `retryAfter` (plus `next.retry`) only when the reset or `retry-after` falls after the tool call's own deadline; no fixed wait cap or client-side window decides. `retry-after` wins over the reset header. Waiting does not spend the request timeout. While a wait of a second or more lasts, the call reports it (the limit, seconds left, and why: quota reset or `retry-after`, secondary cooldown, or search pacing) when it starts and every 10 s after: an MCP client that sent `_meta.progressToken` receives each line as `notifications/progress` before the result, so a host that resets its timeout on progress keeps the call alive; a CLI call prints the same lines on stderr (through a warm server, with the call's other notices once it ends). Search and code-search pacing follow the quota GitHub reports for each resource (`x-ratelimit-resource`, `-remaining`, `-reset`): planned searches the remaining quota covers go back-to-back, and only searches beyond it are spread over the time until reset. Until a response reports the `code_search` quota, planning assumes GitHub's documented default for authenticated code search, 10 a minute ([search rate limit](https://docs.github.com/en/rest/search/search#rate-limit)); this is an assumption, not a cap, and the first response replaces it (so does any quota window that has ended). Processes sharing a token share the last quota GitHub reported and the start of each paced search through one file lock on `tmp/ratelimit/` (keyed by host and a token hash, never the token); a start stamp dated more than two minutes ahead is ignored, so a crashed process never wedges it. If the file or its directory cannot be read, written or locked, the call goes ahead under this process's own pacing and one warning goes to stderr.
- **Classification gate:** one process-wide gate per provider endpoint allows 10 requests in flight; one tool call may use up to three quarters. Throttling (429/503/529) halves the gate, `Retry-After` pauses every caller, and the gate recovers gradually.
- **Context selection:** units the budget leaves unassessed stay unknown and never spill into the default evidence body; recovery: [context selection](OCTOCODE_TOOLS.md#context-selection).

## See also

- [Configuration](CONFIGURATION.md)
- [Security](SECURITY.md)
- [Documentation hub](README.md)
