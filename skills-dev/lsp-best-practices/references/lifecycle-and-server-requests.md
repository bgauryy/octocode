# Lifecycle & Server Requests: initialize to exit

Load when changing startup, shutdown, the capabilities we declare, or how the client answers server→client requests. Why: a server that sends a request and never gets an answer waits forever. A capability we declare but don't handle makes the server send result shapes we can't parse.

## Sequence
1. **Spawn** the server in its own process group with piped stdio and a cleared environment plus an allowlist. Start draining stderr in its own task immediately.
2. **`initialize`.** This request must be the only thing on the wire until its response arrives. Either queue other requests inside the transport until init completes (Helix) or fail them fast with -32002 (tower-lsp-server). Params:
   - `processId`: our pid. Well-behaved servers exit when it dies, which is a free guard against orphans.
   - `clientInfo {name, version}`.
   - `workspaceFolders`, **plus** `rootUri` for older servers.
   - `capabilities`: **only what we actually handle** (see below).
   - `initializationOptions`: server-specific settings. Load `references/servers.md` for each server.
3. **Check the response.** Store `capabilities` once (for example in a `OnceCell`). Verify that `positionEncoding` is one we speak; if it is missing, assume utf-16.
4. **`initialized`** notification. Only after this do `didOpen` and queries go out.
5. **Stop.** Send a `shutdown` request with a short timeout, then the `exit` notification. Wait a bounded time for the process to exit on its own, then kill the process group and reap it. The server exits with code 0 if `shutdown` came first and 1 otherwise. Never block quitting on a slow server; Helix's example is gopls flushing about 1k log lines before it answers `shutdown`.

## Capabilities: declare what you parse
| Declare | Only if the client handles |
|---|---|
| `definition.linkSupport` | `LocationLink[]` (use `targetSelectionRange` for identity) |
| `documentSymbol.hierarchicalDocumentSymbolSupport` | nested `DocumentSymbol[]` |
| `window.workDoneProgress` | `workDoneProgress/create` and `$/progress` tracking |
| `publishDiagnostics.versionSupport` | versioned diagnostics, **and** accepts unversioned ones (TS sends none) |
| `general.staleRequestSupport {cancel, retryOnContentModified}` | ContentModified retry. It tells servers we retry. |
| `synchronization.didSave` | actually sending `didSave`. Don't declare it if we never do. |
| `experimental.serverStatusNotification` (rust-analyzer) | `experimental/serverStatus` quiescent tracking |
| `dynamicRegistration` | honoring `client/registerCapability`. A read-only client usually sets `false` everywhere. |

## Server→client requests: the must-answer table
| Method | Minimal correct answer |
|---|---|
| `workspace/configuration` | An array with **one entry per `items[i]`**, resolved by `items[i].section` (for example `"rust-analyzer"`, `"gopls"`, `"pylsp"`), or `null` when unknown. Don't send the whole settings blob for every item. |
| `client/registerCapability` / `unregisterCapability` | `null`. **Never** MethodNotFound: servers built on vscode-languageserver-node treat the rejection as fatal and exit (Helix comment). |
| `window/workDoneProgress/create` | `null`, and start tracking the token |
| `workspace/workspaceFolders` | the folder list |
| `window/showMessageRequest` | `null` |
| `workspace/applyEdit` | `{applied:false}` for a read-only client |
| `workspace/*/refresh` (semanticTokens, inlayHint, diagnostic, codeLens) | `null`. If diagnostics are cached, invalidate them. |
| anything else | `-32601`. Never `result:null`, which tells the server the request was handled. |

Answer through the writer queue, never with an inline blocking write from the read loop.

Next: for who owns the process and pending map load `references/handles.md`. For per-server settings, `references/servers.md`.
