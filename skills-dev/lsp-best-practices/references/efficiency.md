# Efficiency: pool, documents, batching, bounds

Load when a call is slow, memory grows, or you're tuning pooling, document handling, or request concurrency. Why: cold start dominates everything. On this repo the first rust-analyzer call took about 47 s (with ~45 s of system CPU) and a warm call about 6 s. Every avoidable restart or re-index costs far more than any micro-optimization.

## Levers, biggest first
| Lever | Rule |
|---|---|
| **Server reuse** | Pool per `(canonical root, command, args, env, init options, memory cap)`. Deduplicate concurrent starts, evict on an idle TTL plus LRU, and never spawn per query. Anything that changes the key (for example `rustContext`) is a new cold start, so keep variants rare. |
| **Headless settings** | Turn off work a read-only client never uses: `cargo check`, build scripts, proc-macros, linters, typings download, background indexes that write into the repo. Load `references/servers.md`. |
| **Batch on one anchor** | Several operations against one opened document share the server, the open, and the warm caches. |
| **Documents** | Open each document once. Use `didChange` with a bumped version when content changes. Keep an LRU of open documents that sends `didClose` on eviction, and remember that definition-hop targets use slots too. Never `didOpen` twice. |
| **Pipelining** | Send independent requests concurrently under a bound, and match responses by id. |
| **Result caps** | Cap references, workspace/symbol, walk nodes, and partial-result merges before serializing. Page with a stable snapshot. |
| **Queries** | Avoid `workspace/symbol` with empty or very short queries: csharp-ls returns every declaration, and clangd without an index returns only open files. |

## Memory
- Use an OS cap on the child: `RLIMIT_AS` via `pre_exec` on Linux, and a Job Object on Windows (which also does the tree kill). **Not on macOS**, where `RLIMIT_AS` makes every spawn fail with EINVAL.
- Use server-side knobs as well: rust-analyzer `lru.capacity` and `numThreads`, tsserver `maxTsServerMemory`, the jdtls JVM `-Xmx`, clangd `--pch-storage=memory` and `-j`.
- Pool entry cap × the heaviest server's footprint must fit the host. rust-analyzer, tsserver, and jdtls each reach GBs on big repos.

## Measure, don't guess
- Time the real surface, cold and warm: `time $OCTO lspSearch '<json>'`. The first call includes spawn, index, and readiness; later calls show steady state.
- Separate the phases with `debug:true` (readiness status, server info), and compare before and after on the same repo and anchor.
- A win only counts if both cold and warm improve (or one improves and the other holds) and the answers are identical.

Next: for the per-server settings load `references/servers.md`.
