# Readiness: indexing waits and retries

Load when a result is empty or partial on a cold server, when changing readiness waits or retry policy, or when adding a new server. Why: servers index in the background. The client has to tell "not indexed yet" apart from "none", or it reports absence that isn't real. Claude Code issue #44767 is the canonical failure: gopls's "Loading packages…" became "No definition found".

## Signal ladder (use the strongest one the server offers)
1. **A server-specific ready signal:** rust-analyzer `experimental/serverStatus {quiescent:true}` (only if the client opts in with `experimental.serverStatusNotification`), or jdtls `language/status ServiceReady`. Per-server details are in `references/servers.md`.
2. **The `$/progress` token set:** ready means at least one `begin` was seen, no tokens are still open, and a short quiet window has passed; a new wave restarts the wait. This requires declaring `window.workDoneProgress` and answering `workDoneProgress/create`. clangd's progress stays stuck otherwise.
3. **First-open settle:** subscribe to progress **before** `didOpen` so an early begin/end pair isn't missed, then wait a bounded settle window. This is the only option for silent servers (typescript-language-server, pylsp).
4. **Nothing:** proceed after the settle window and label the result's readiness honestly.

## Rules
- **Every wait has a deadline**, and the caller's deadline is longer than it. Serena waits up to 120 s for quiescent and then proceeds anyway. Waits without a timeout hung its startup.
- **Readiness is a soft gate.** rust-analyzer's docs say clients shouldn't rely on health status to decide whether to send requests. jdtls `ServiceReady` fires before its build jobs finish.
- **Empty on a non-ready server means "unknown"**: retry once after ready. Never cache an empty result (Serena caches only non-empty document symbols for this reason).
- **Retry only typed staleness:** ContentModified, and ServerCancelled with `retriggerRequest`, with small backoff and a cap. Match on the error **code**, never on message text. No broad sleep-and-retry: it hides cycles and real failures.
- **Label the output:** `progressIdle | settledWithoutProgress | timeout`, plus a result-level `indexing` / partial flag. `settledWithoutProgress` is normal for silent servers.

## Per-language budgets (octocode `engine/src/lsp/pool.rs` `readiness_timeout`)
| Language | Budget |
|---|---|
| java | 120 s |
| rust | 60 s |
| TS/JS, csharp, swift | 30 s |
| c/cpp/cuda | 20 s |
| go, python | 15 s |
| shell | 2 s |
| anything else | **none** (no readiness wait at all) |

A readiness timeout doesn't fail the start (`NativeLspClient::wait_for_ready` returns `"timeout"`): the server is pooled, and results are marked partial.

Next: for per-server signals and settings load `references/servers.md`. For what to do with the time you save, `references/efficiency.md`.
