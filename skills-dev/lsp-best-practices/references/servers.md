# Servers: per-server readiness, settings, gaps

Load when adding a server route, changing `initializationOptions` or `workspace/configuration` answers, debugging one server's behavior, or tuning cost for headless read-only use. Why: each server has its own readiness signal, config shape, capability gaps, and a habit of running repository code. Generic client code gets at least one of those wrong. Verified September 2026 against server source and docs; re-check before relying on an exact key.

## `workspace/configuration`: reply with the section, not the blob
Reply with **one entry per item, in order**. Each entry is `get(settings, item.section)`, walking dotted paths, or `null`. Sending the whole `initializationOptions` blob for every item is wrong for every server that pulls configuration.

| Server | Pulls? | Sections | Effect of the full blob |
|---|---|---|---|
| rust-analyzer | only after `didChangeConfiguration` | `rust-analyzer` | settings ignored or flagged as errors |
| gopls | at init (per folder) and on change | `gopls` | unknown keys → error messages |
| pyright / basedpyright | if `workspace.configuration` is declared | `python`, `python.analysis`, `pyright` / `basedpyright…` | silently falls back to defaults |
| csharp-ls | yes | `csharp` | deserialization fails → None |
| metals | yes (per folder) | `metals` | same class of problem |
| jdtls | formatting only | `java.format.*` (scalars) | a blob where a number is expected |
| clangd, pylsp | no (they read init options or `didChangeConfiguration`) | n/a | n/a |

## Per-server profile
| Server | Ready signal | Headless settings (read-only) | Gaps and quirks |
|---|---|---|---|
| rust-analyzer | `experimental/serverStatus` quiescent (**opt-in**) | `checkOnSave:false`, `cargo.buildScripts.enable:false`, `procMacro.enable:false`, `cachePriming.enable:false`, `files.exclude`, `lru.capacity`, `numThreads`, `cargo.targetDir` | **By default it runs build.rs, proc-macros, and `cargo check`.** No standard typeHierarchy. Root = `Cargo.toml`/`rust-project.json`. |
| typescript-language-server | none (silent). The project loads on the first `didOpen`. | `maxTsServerMemory`, `disableAutomaticTypingAcquisition:true`, `tsserver.useSyntaxServer:"never"` | **No typeHierarchy.** callHierarchy only if the client declares it. workspace/symbol needs an opened file. |
| gopls | `$/progress` "Setting up workspace" → "Finished loading packages." | `directoryFilters:["-node_modules","-vendor"]`, `expandWorkspaceToModule:false` (`memoryMode` is obsolete) | Runs `go list`, which needs the toolchain. Can take minutes on big repos. |
| pylsp (octocode fallback) | none (jedi is lazy) | disable linter plugins (`pycodestyle`, `pyflakes`, `mccabe`) | **No callHierarchy, typeHierarchy, workspaceSymbol, or implementation.** |
| pyright / basedpyright | `$/progress` during analysis | `diagnosticMode:"openFilesOnly"`, `analysis.exclude` | callHierarchy yes, typeHierarchy no. `pyrightconfig.json` overrides client settings. |
| clangd | `$/progress` `backgroundIndexProgress` "indexing" (you must answer `workDoneProgress/create`) | `--background-index=false` (it otherwise **writes `.cache/clangd` in the repo**), `-j`, `--pch-storage=memory`, `--log=error`, `--clang-tidy=false`, `--compile-commands-dir` | Needs `compile_commands.json`. outgoingCalls needs index support. |
| jdtls | `language/status` `ServiceReady` (fires **before** build jobs finish) | `-Xmx`, a **unique `-data` dir per workspace**, `java.autobuild.enabled:false`, no source downloads | Gradle/Maven import **runs build logic**. Heavy JVM. |
| csharp-ls | `$/progress` "Loading N project(s)" | `--solution`/`solutionPathOverride`, `analyzersEnabled:false` | workspace/symbol is truncated at 100, and an empty query returns everything. Answers are empty until the solution loads. MSBuild **runs targets**. |
| metals | `metals/status` + `$/progress` | `autoImportBuilds:"initial"`, `isHttpEnabled:off` | Writes `.metals/` and `.bloop/`, and compiles. The least read-only-friendly server here. |

## Security posture
rust-analyzer (build.rs, proc-macros), jdtls (Gradle/Maven), csharp-ls (MSBuild), and metals (BSP compile) **run repository code by default**. Only rust-analyzer can fully switch this off through settings. For a research tool over untrusted checkouts, make "no code execution" the default and require an explicit opt-in (octocode's `rustContext.buildScripts`/`procMacros`), **including when no context is passed**.

Next: for how readiness feeds result labeling load `references/readiness.md`. For octocode's defect history, `references/octocode-known-defects.md`.
