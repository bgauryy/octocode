# References: sources behind this skill

Load when a rule is disputed or needs re-verification against its origin. Why: protocol rules come from the spec, server behavior changes between releases, and octocode anchors drift after refactors.

## Specification and server docs
| Source | Location | Used for |
|---|---|---|
| LSP 3.17 spec | https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/ | framing, errors, lifecycle, sync, positionEncoding, hierarchy `data`, pull diagnostics, staleRequestSupport |
| rust-analyzer book | rust-analyzer.github.io/book (`configuration.html`, `contributing/lsp-extensions.html`) | settings, serverStatus quiescent |
| gopls | golang/tools `gopls/doc/settings.md`, `gopls/internal/server/general.go` | config pull, progress titles, directoryFilters |
| typescript-language-server | `docs/configuration.md`, `src/lsp-server.ts` | init options, callHierarchy gating, workspace/symbol |
| pyright / basedpyright | microsoft/pyright `server.ts`; docs.basedpyright.com | config sections, diagnosticMode |
| pylsp | python-lsp/python-lsp-server `pylsp/python_lsp.py` | capability gaps |
| clangd | llvm-project `clang-tools-extra/clangd/ClangdLSPServer.cpp` | progress token, flags, `.cache` writes |
| jdtls | eclipse-jdtls `ServiceStatus.java`, `Preferences.java` | ServiceReady, `-data`, build execution |
| csharp-ls / metals | razzmatazz/csharp-language-server; scalameta/metals | config sections, truncation, workspace writes |

## Client implementations
| Repo | Path | Lesson |
|---|---|---|
| zed-industries/zed | `crates/lsp/src/lsp.rs`, `input_handler.rs` | cancel-on-drop guard, bounded inbound queue, `Option`-taken pending map |
| helix-editor/helix | `helix-lsp/src/{client,transport,lib}.rs` | queue until init, synchronous id, reply OK to registerCapability, force_shutdown |
| rust-lang/rust-analyzer | `lib/lsp-server/src/stdio.rs` | rendezvous channels, dropper thread |
| oxalica/async-lsp | `src/concurrency.rs`, `src/lib.rs` | concurrency cap, outgoing-first priority |
| tower-lsp-community/tower-lsp-server | `src/service/{pending,client}.rs` | abortable pending, -32002 before init |
| gluon-lang/lsp-types, gen-lsp-types | `Cargo.toml` | Uri type churn |
| oraios/serena | `src/serena/tools/symbol_tools.py`, `src/solidlsp/ls.py` | name-path anchors, output tiers, empty-not-cached, readiness waits |
| isaacphi/mcp-language-server | `internal/tools/*.go` | contrast: empty-as-success, absolute paths |
| tower-lsp/tower-lsp | `src/codec.rs` | cancel-safe `Decoder`, resync scan, small-chunk tests (no max length) |
| watchexec/process-wrap | `process_wrap::tokio` | group/session/Job Object with suspended assign; successor of the deprecated command-group |
| zed `FakeLanguageServer` | `crates/lsp/src/lsp.rs` (`test-support`) | scripted fake server API |
| microsoft/multilspy | `src/multilspy/language_server.py` | ref-counted open/close |

## Rust runtime docs
| Source | Used for |
|---|---|
| docs.rs tokio `AsyncReadExt`/`AsyncBufReadExt`/`process::Command` | cancel-safety table, `process_group` (tokio 1.40), `kill_on_drop` caveats |
| docs.rs tokio-util (`CancellationToken`, `AbortOnDropHandle`, `TaskTracker`, codec) | task ownership, codecs |
| serde_json `RawValue` | lazy result parsing |

## Evidence on agents
| Source | Finding |
|---|---|
| arXiv 2608.13568 "Does a Language Server Save Tokens for Coding Agents?" | route by task: LSP for references, grep for localization and renames |
| anthropics/claude-code #44767 | readiness notification reported as "no definition" |
| Cursor semsearch blog; Aider repomap docs | combining search surfaces wins; signatures under a budget |

## Local
| Path | Notes |
|---|---|
| `packages/octocode-native/crates/engine/src/lsp/`, `crates/runtime/src/tools/lsp_search/` | the client and the tool |
| `packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md`, `LSP_AST_AUDIT_FINDINGS.md` | resolution, provisioning, open P1s |
| `.octocode/GOTCHAS.md` (LSP lines) | anchors, TS readiness, TS type hierarchy |
| Code review 2026-09-24 (subagent, read-only) | Rust findings R1–R18, house patterns |
| Dogfood 2026-09-24 (`$OCTO lspSearch` on `pool.rs`) | 47 s cold / 6 s warm; mixed line numbering; flat depth-2 walk |
