Load when serving or replaying the viewer, or reading `events.jsonl`.

# Viewer, server and events

## Viewer and server

Chat from the viewer, replies, a refresh after code changes and the endpoint rules: [chat](chat.md). A standalone HTML page: [viewer delivery](../output.md#viewer-delivery).

Event batches travel as lossless gzipped WebSocket frames, decoded in arrival order. `octocode-graph serve` binds 127.0.0.1 on a random (or `--port`) port, serves only its own assets, and requires the run token in the page and WebSocket URLs plus a same-server `Origin`. The pipeline starts with the server; a reconnecting page sends `hello {since}` to recover missed events. The server remains available after `done` for AST outlines (`astSearch symbols`) and LSP `callers`, `callees` or `references`; those answers are events too.

Each line of `events.jsonl` records an event with a sequence number and elapsed milliseconds. `octocode-graph events <dir>` summarizes it: per tool and in total, the calls (each tool id once), cursor pages, failed calls, calls answered with an error row, unfinished calls, rows, bytes and summed call time; per run (the build, each refresh, then later events), the wall time from its first event to its last and each phase's duration; and whether every id started exactly once. `--json` writes every field.

| type | Carries |
|---|---|
| `run`, `phase`, `progress` | Root and options; phase start/done/skipped with notes; done/total per phase |
| `skeleton`, `plan`, `chunk` | Listed paths per page; planned chunks; chunk state (ingesting, paging, done, split, error) with counts, and `build`: the latest graphBuild progress line while ingesting |
| `rows` | `files`, `witnesses` or `symbols` rows of one graph page, root-relative |
| `findings` | One chunk's ranked issues, cycles, health, detector counts |
| `stitched`, `graph-ready`, `lsp` | Cross-chunk imports and calls, package nodes, gaps; per-batch call provenance updates and LSP-only calls |
| `tool` | Every octocode call: tool, detail, page (first or cursor), rows, total, continues, ms, bytes, error |
| `metrics`, `done`, `error`, `note` | Per-file metric columns aligned to one id list, weighted edges as index columns, cycles, communities, packages, timings; result paths; failures; warnings |
| `ast`, `lspResult` | On-demand answers from the page |

A viewer connection resumes after its last applied sequence number. Replay and live traffic use ordered gzip batches sized to the socket write target. A larger event travels whole. A slow connection resumes as its socket drains.
