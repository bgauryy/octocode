# LSP in Octocode — Lifecycle, Provisioning, and the No-Fallback Contract

Companion: `docs/context/LSP_GUIDE.md` (protocol primer + platformized resolution ladder).

---

## Two layers, two different jobs

| Layer | What it is | Speed | Answers |
|---|---|---|---|
| **Tree-sitter / OXC** | parser compiled into the engine | sub-ms/file | *"what does this code look like?"* — outlines, shapes, calls, imports (`structural/`, `signatures/`, `grammar.rs`) |
| **LSP** | a real language server, spawned over stdio | cold 1–120s, warm <100ms | *"what does this symbol mean?"* — cross-file definition identity, all references, call/type graph (`lsp/client.rs`, `manager.ts`, tools-core `semantic_content/execution.ts`) |

These are **not** interchangeable. Tree-sitter cannot resolve a symbol across files, infer a type, or follow an import — that needs a language server.

## The no-fallback contract (the rule that matters)

When a semantic operation needs a language server and **no server is available**, octocode **throws** — it does *not* fabricate a syntactic or same-file approximation. An honest failure prevents the calling agent from trusting incomplete syntax evidence as a semantic answer.

- The thrown error is the standard typed envelope: `status:"error"`, `errorCode:"lspServerUnavailable"`. In bulk it lands under `errors[]`.
- The message names the language, says no server is available, gives the install hint, and **directs the agent to `localSearch` (text/structural operation) + `localGetFileContent`** instead.
- octocode never returns a same-file-only `references` result, or a tree-sitter guess, dressed up as a semantic answer.

**Throws when no server:** `definition`, `references`, `hover`, `callers`, `callees`, `callHierarchy`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, `diagnostic`.

**Never throws (genuine tree-sitter features, server-free):** `documentSymbols` (native OXC for JS/TS, Markdown heading outline, or LSP when present) and structural/AST search via `localSearch.operation:"structural"`. These are real syntactic capabilities, not LSP stand-ins. `documentSymbols` only throws for a non-JS/TS language with no server *and* no outline.

> A server that *is* running but lacks a capability, or returns zero results, still yields an honest *empty* (`unsupportedOperation` / `noReferences` / …) — that is an accurate answer ("none"), not a missing-server failure.

## Server availability classes

octocode resolves servers through the ladder override → PATH → bundled →
ecosystem discovery → managed cache; see `LSP_GUIDE.md` section 13. Use
`npx octocode lsp-server list` for managed-download and toolchain servers,
`npx octocode lsp-server status FILE_PATH` for the full resolution result for a
specific file, and `npx octocode lsp-server install SERVER_NAME` to trigger a
supported managed download. Servers fall into:

- **Bundled (npm dep, offline)** — servers launched with the current Node; zero separate server install. TS/JS, Python (pyright), Bash, YAML, JSON/HTML/CSS. Bash uses its packaged WASM parser; ShellCheck and shfmt are optional linting/formatting additions.
- **Auto-download (managed cache)** — portable single-binary servers fetched from a pinned release into `~/.octocode/lsp/<server>/<tag>/` (prompt-by-default, SHA-verified). rust-analyzer (all platforms), clangd (no linux-arm64 asset). Set `OCTOCODE_LSP_AUTO_INSTALL=auto` to skip the prompt or `=off` to turn managed downloads off.
- **Detect-and-instruct (host toolchain)** — need a runtime or server that octocode doesn't auto-install: intelephense (PHP), gopls (Go), jdtls (JDK 21+), sourcekit-lsp (Xcode/CLI tools on macOS), csharp-ls (.NET SDK). The status/hint tells you how to install; semantic ops throw until you do.
- **Resolve-if-installed** — known server commands for Ruby, Kotlin,
  Elixir, Scala, and SQL resolve from `PATH`, ecosystem locations, or an explicit
  environment override. Octocode does not install these servers.

## Supported language servers

Built-in routing covers bundled, managed, host-toolchain, and resolve-if-installed
servers. A file type not listed below can still use a custom server configuration.
Without built-in or custom routing, semantic operations return
`lspServerUnavailable`; use text or structural search for discovery.

| Language | Extensions | Server | Provisioning |
|---|---|---|---|
| TypeScript / JS (+ TSX/JSX) | `.ts .mts .cts .tsx .js .mjs .cjs .jsx` | typescript-language-server (`tsgo`/override aware) | **bundled** |
| Python | `.py .pyi` | pyright (`pylsp` via override) | **bundled** |
| Shell | `.sh` | bash-language-server | **bundled** (or `OCTOCODE_BASH_SERVER_PATH`) |
| PHP | `.php` | intelephense | **detect-and-instruct** (project, `PATH`, or `OCTOCODE_PHP_SERVER_PATH`) |
| YAML | `.yaml .yml` | yaml-language-server | **bundled** |
| JSON | `.json .jsonc` | vscode-json-language-server | **bundled** |
| HTML | `.html .htm` | vscode-html-language-server | **bundled** |
| CSS / SCSS / Less | `.css .scss .less` | vscode-css-language-server | **bundled** |
| Rust | `.rs` | rust-analyzer | **auto-download** |
| C / C++ | `.c .h .cpp .cc .cxx .hpp .hh .hxx` | clangd | **auto-download** (no linux-arm64 asset) |
| Go | `.go` | gopls | **detect-and-instruct** (needs Go toolchain) |
| Java | `.java` | jdtls | **detect-and-instruct** (needs JDK 21+) |
| Swift | `.swift` | sourcekit-lsp | **detect-and-instruct** (needs Xcode or `xcode-select --install`) |
| C# | `.cs` | csharp-ls | **detect-and-instruct** (needs .NET SDK + `dotnet tool install -g csharp-ls`) |
| SQL | `.sql` | sqls | **PATH / override only** |
| Ruby | `.rb .rake .gemspec .ru` | ruby-lsp | **PATH / override only** |
| Kotlin | `.kt .kts` | kotlin-language-server | **PATH / override only** |
| Elixir | `.ex .exs` | elixir-ls | **PATH / override only** |
| Scala | `.scala .sc` | metals | **PATH / override only** (requires a supported JDK) |

The bundled TypeScript server starts with `tsserver.useSyntaxServer:"never"` so
definition, hover, and related requests use the full semantic project from the
first request. Startup may take longer, but import aliases, re-exports, and
tsconfig path aliases resolve without Octocode rewriting provider locations.

Any built-in server can be overridden with its language-specific
`OCTOCODE_*_SERVER_PATH` variable or `.octocode/lsp-servers.json`.
PATH/override-only servers resolve only if already on `PATH` / in an ecosystem dir; otherwise
semantic ops throw with an install hint.

Native grammar availability and external server resolution are independent. See
[Supported languages and features](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/SUPPORTED_LANGUAGES_AND_FEATURES.md) for
the structural-search set. Shell, Less, and Elixir are LSP-only routes. Scala
uses Metals when installed. Files
without a registered grammar remain searchable with `localSearch
operation:"text"`.

`octocode lsp-server list` reports managed-download and toolchain-required
servers, including the resolve-if-installed rows above, and prints a note naming
the bundled servers. Use `octocode lsp-server status FILE_PATH` for
the authoritative resolution result for one file.

Discovery caches expire after five seconds, including missing-server results
and ecosystem-directory inventories. A long-running MCP session can discover a
server installed during the session without restarting. `clearDiscoveryCache()` refreshes
immediately for callers that manage installation themselves. Availability means
the launcher resolved; startup can still fail if its host toolchain is missing.

### Custom / bring-your-own LSP (any language)

A language with **no built-in spec** can use semantic support by
registering a server in a JSON config — no rebuild, no code change. This is also how you swap a
built-in server for a different one. Resolution reads, in order (`config.rs::user_config_paths`):

1. `$OCTOCODE_LSP_CONFIG` (explicit file path)
2. `<workspace>/.octocode/lsp-servers.json` (per-project, checked in or local)
3. `~/.octocode/lsp-servers.json` (per-user, all projects)

The file maps a **file extension** to a launch spec. A custom entry takes precedence over the
built-in spec for that extension:

```jsonc
// .octocode/lsp-servers.json — override the Scala launch configuration
{
  "languageServers": {
    ".scala": { "command": "metals", "args": [], "languageId": "scala" },
    ".sc":    { "command": "metals", "args": [], "languageId": "scala" }
  }
}
```

| Field | Required | Meaning |
|---|---|---|
| `command` | yes | Executable name (resolved on `PATH`) or absolute path. Shell wrappers are rejected. |
| `languageId` | yes | LSP `languageId` sent on `textDocument/didOpen` (for example, `scala` or `ruby`). |
| `args` | no | Launch args (default `[]`). |
| `initializationOptions` | no | Passed verbatim in the LSP `initialize` request. |

With the config present, the server can answer the semantic operations it
advertises. **Without it, the extension stays unsupported:
the engine resolves no server and the no-fallback contract applies** — semantic ops throw
`lspServerUnavailable` and the agent falls back to `localSearch` + `localGetFileContent`.
Verify custom routing with `octocode lsp-server status FILE_PATH`, then run a
`documentSymbols` request followed by an anchored operation such as `definition`
or `references`.

### Markup and docs: what's LSP vs minify

- **HTML / CSS / SCSS / Less / JSON / YAML are LSP** — served by the bundled
  `vscode-*-language-server` / `yaml-language-server` (markup/data, offline-ready). They're
  not "code" languages but they do have real language servers.
- **Markdown / MDX are NOT LSP.** They are handled by the **minifier** using heading-section
  heuristics (ATX `#`/`##` and setext headings → `minify/strategies/markdown.rs`; `md`,
  `markdown`, `mdx` all map to the markdown strategy in `minify/config.rs`). There is no
  markdown language server in octocode — `documentSymbols` on a `.md` file uses the native
  heading-outline path, and structure/compression comes from the minifier, not a server.

## Full format support matrix

See [Supported languages and features](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/SUPPORTED_LANGUAGES_AND_FEATURES.md) for
structural search and signature coverage, including commands that inspect the
compiled engine. The [supported language servers](#supported-language-servers)
table above describes external server resolution, which is independent of native
grammar availability.

The former per-extension matrix duplicated the grammar registry and referenced a
retired benchmark generator. Use `getSupportedStructuralExtensions()`,
`getSupportedSignatureExtensions()`, `getMINIFY_CONFIG()` and
`getLanguageServerForFile(file, workspace)` for the current build's capabilities.

## Lifecycle — pool, cold start, indexing

- **Pool** (`lspClientPool.ts`): one warm `LSPClient` per server, workspace, and effective configuration fingerprint, with a 60s idle timeout (`OCTOCODE_LSP_POOL_IDLE_MS`). A long-lived MCP session reuses warm servers across tool calls; one-shot CLI invocations don't share a pool.
- **Native contract**: the TypeScript wrapper and native addon ship together. Lifecycle, capability, readiness, and health methods are required. Failed health checks evict the client so the next acquisition starts a replacement; missing methods on a stale addon are errors.
- **Cleanup during startup**: clearing a key or the pool invalidates pending acquisitions immediately. A client created after its acquisition was invalidated is stopped and the acquisition returns `null`. Cleanup does not wait for a pending factory to finish. Stale startup and health-check completions cannot replace or remove a newer acquisition.
- **Cold start / indexing**: a server reads the project and builds its model before answering correctly. Costs vary — typescript-language-server <1s, gopls 3–15s, rust-analyzer 5–60s (multiple `$/progress` waves), jdtls 30–120s.
- **Readiness** (`manager.ts` + `json_rpc.rs`): for servers that emit `$/progress` (go, rust, java, csharp, swift) the pool factory calls `waitForReady` with a per-language cap before the first query. Bash also uses a bounded 2s settle because it loads client configuration asynchronously before enabling document analysis; this records `settledFallback`, not confirmed indexing. TS/JS, Python, clangd, and data-format servers skip the settle interval.
- **Spawn gate**: every resolved command passes `validateLSPServerPath` (rejects shell wrappers / nonexistent / non-executable) in `LSPClient.start()` before the process is spawned.
- **Discovery caching** (`serverDiscovery.ts`): lookup results and ecosystem-directory inventories expire after five seconds, including missing-server results. Call `clearDiscoveryCache()` to refresh immediately after installing a server during a session.

## LSP indexing limits

After progress tokens drain, readiness requires a full 200 ms interval without
progress updates, including when a cycle completed before the wait began. A new
update restarts that interval. If the deadline prevents a full quiet interval,
readiness is `timeout`, not `progressIdle`. A server that never reports progress
uses `settledFallback` after the bounded settle interval. Neither fallback nor
timeout confirms that indexing finished; affected empty semantic answers expose
`partialReasons: ["readinessUnconfirmed"]` and a warning. A quiet progress interval
does not freeze the server's model or prove complete compiler coverage.

Project-wide operations run a bounded lexical consumer warmup before the LSP
request. The warmup follows search pages, opens up to 100 candidate files, and
records capped searches, skipped files, failed reads, and broken continuations.
`references`, `implementation`, `callers`, `callees`, and `callHierarchy`
preserve this state in `payload.warmup` for structured and compact output. An
incomplete warmup sets typed partial metadata and supplies
`next.verifyCompleteness` for a workspace-wide lexical cross-check.

Servers without `$/progress`, including `typescript-language-server`, can still
change their indexed result set after a request. Semantic pagination sorts and
deduplicates provider results and adds a snapshot fingerprint to continuations.
Later pages reject a changed result set and supply a restart call, preventing
pages from different result sets from being combined. This does not freeze the
server's index. A partial warmup or readiness state prevents using zero
project-wide results as absence evidence. Even without these states, results
remain scoped to the provider, project configuration, and supported operations. The
semantic evidence and continuation workflow is documented in
[the LSP guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#lsp-tools-reference).

## Rust context and server identity

The public `rustContext` query option flows through `manager.ts` to
`rustContext.ts`, which maps it to rust-analyzer initialization options. The
context controls Cargo features, default features, target, cfg settings, build
scripts, and procedural macros. Explicit contexts default both executable
providers to disabled, disable implicit test cfg and check-on-save, and use
rust-analyzer's dedicated Cargo target directory. Procedural macros require build
scripts to be enabled because rust-analyzer uses build-script processing to load
them. Enabling either provider allows workspace code execution; no sandbox is
provided. Omitting the context preserves existing server configuration defaults.

Pool keys include a canonical fingerprint of the effective initialization options
and explicitly configured server environment. Equivalent feature and cfg lists
reuse a client; different build contexts cannot share one. Acquisition and release
helpers accept the same optional context. Public result fingerprints cover the
normalized requested Rust context, and pagination additionally fingerprints the
semantic query and complete provider result set. A context change invalidates a
continuation even if the returned locations happen to match.

These identities do not pin the compiler version, inherited process environment,
Cargo configuration on disk, source bytes, or generated files. The public context
remains visible if document symbols come from a native syntax provider, but does
not turn that provider or the file graph into a compiler expansion. See the
[public fields and example](../../../docs/OCTOCODE_TOOLS.md#rust-build-context)
and the upstream [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).
