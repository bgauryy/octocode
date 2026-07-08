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

When a semantic operation needs a language server and **no server is available**, octocode **throws** — it does *not* fabricate a syntactic or same-file approximation. A faked answer is worse than an honest failure, because the calling agent would trust it.

- The thrown error is the standard typed envelope: `status:"error"`, `errorCode:"lspServerUnavailable"`. In bulk it lands under `errors[]`.
- The message names the language, says no server is available, gives the install hint, and **directs the agent to `localSearchCode` (text/structural search) + `localGetFileContent`** instead.
- octocode never returns a same-file-only `references` result, or a tree-sitter guess, dressed up as a semantic answer.

**Throws when no server:** `definition`, `references`, `hover`, `callers`, `callees`, `callHierarchy`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, `diagnostic`.

**Never throws (genuine tree-sitter features, server-free):** `documentSymbols` (native OXC for JS/TS, Markdown heading outline, or LSP when present) and structural/AST search via `localSearchCode`. These are real syntactic capabilities, not LSP stand-ins. `documentSymbols` only throws for a non-JS/TS language with no server *and* no outline.

> A server that *is* running but lacks a capability, or returns zero results, still yields an honest *empty* (`unsupportedOperation` / `noReferences` / …) — that is an accurate answer ("none"), not a missing-server failure.

## Provisioning — three classes (how a server becomes available)

octocode maximizes the chance a real server answers via the resolution ladder (override → PATH → bundled → ecosystem discovery → managed cache; see `LSP_GUIDE.md` §13). Use `npx octocode lsp-server list` to see all servers and their current status, `npx octocode lsp-server status <file>` to check resolution for a specific file, and `npx octocode lsp-server install <name>` to trigger an auto-download. Servers fall into:

- **Bundled (npm dep, offline)** — pure-JS servers launched with the current Node; zero install. TS/JS, Python (pyright), Shell (bash-language-server), PHP (intelephense), YAML, JSON/HTML/CSS.
- **Auto-download (managed cache)** — portable single-binary servers fetched from a pinned release into `~/.octocode/lsp/<server>/<tag>/` (prompt-by-default, SHA-verified). rust-analyzer (all platforms), clangd (no linux-arm64 asset). Set `OCTOCODE_LSP_AUTO_INSTALL=auto` to skip the prompt or `=off` to disable downloads entirely.
- **Detect-and-instruct (host toolchain)** — need a runtime octocode won't auto-install: gopls (Go), jdtls (JDK 21+), sourcekit-lsp (Xcode/CLI tools on macOS), csharp-ls (.NET SDK). The status/hint tells you how to install; semantic ops throw until you do.

## Supported language servers

Scope is the **main languages**. Niche/long-tail servers were intentionally removed from
LSP routing (their tree-sitter grammars remain for structural/AST search). A file type not
listed below has no server config: semantic ops return `lspServerUnavailable` and the agent
falls back to text search.

| Language | Extensions | Server | Provisioning |
|---|---|---|---|
| TypeScript / JS (+ TSX/JSX) | `.ts .mts .cts .tsx .js .mjs .cjs .jsx` | typescript-language-server (`tsgo`/override aware) | **bundled** |
| Python | `.py .pyi` | pyright (`pylsp` via override) | **bundled** |
| Shell | `.sh` | bash-language-server | **bundled** |
| PHP | `.php` | intelephense | **bundled** |
| YAML | `.yaml .yml` | yaml-language-server | **bundled** |
| JSON | `.json .jsonc` | vscode-json-language-server | **bundled** |
| HTML | `.html .htm` | vscode-html-language-server | **bundled** |
| CSS / SCSS / LESS | `.css .scss .less` | vscode-css-language-server | **bundled** |
| Rust | `.rs` | rust-analyzer | **auto-download** |
| C / C++ | `.c .h .cpp .cc .cxx .hpp` | clangd | **auto-download** (no linux-arm64 asset) |
| Go | `.go` | gopls | **detect-and-instruct** (needs Go toolchain) |
| Java | `.java` | jdtls | **detect-and-instruct** (needs JDK 21+) |
| Swift | `.swift` | sourcekit-lsp | **detect-and-instruct** (needs Xcode or `xcode-select --install`) |
| C# | `.cs` | csharp-ls | **detect-and-instruct** (needs .NET SDK + `dotnet tool install -g csharp-ls`) |
| SQL | `.sql` | sqls | **PATH / override only** |

Any built-in server can be overridden with `OCTOCODE_<LANG>_SERVER_PATH` or `.octocode/lsp-servers.json`.
PATH/override-only servers resolve only if already on `PATH` / in an ecosystem dir; otherwise
semantic ops throw with an install hint.

**Removed from LSP routing** (use text/structural search instead): TOML, Ruby, Kotlin,
Elixir, Terraform, Lua, Proto, OCaml, Zig, Julia, Erlang, R, GDScript. Their tree-sitter
grammars stay available for `localSearchCode` structural/AST queries.

### Custom / bring-your-own LSP (any language)

A language with **no built-in spec** (e.g. Scala, Kotlin, Ruby) gets full semantic support by
registering a server in a JSON config — no rebuild, no code change. This is also how you swap a
built-in server for a different one. Resolution reads, in order (`config.rs::user_config_paths`):

1. `$OCTOCODE_LSP_CONFIG` (explicit file path)
2. `<workspace>/.octocode/lsp-servers.json` (per-project, checked in or local)
3. `~/.octocode/lsp-servers.json` (per-user, all projects)

The file maps a **file extension** to a launch spec. A custom entry takes precedence over the
built-in spec for that extension:

```jsonc
// .octocode/lsp-servers.json — register Scala (metals)
{
  "languageServers": {
    ".scala": { "command": "metals", "args": ["stdio"], "languageId": "scala" },
    ".sc":    { "command": "metals", "args": ["stdio"], "languageId": "scala" }
  }
}
```

| Field | Required | Meaning |
|---|---|---|
| `command` | yes | Executable name (resolved on `PATH`) or absolute path. Shell wrappers are rejected. |
| `languageId` | yes | LSP `languageId` sent on `textDocument/didOpen` (e.g. `scala`, `ruby`). |
| `args` | no | Launch args (default `[]`). |
| `initializationOptions` | no | Passed verbatim in the LSP `initialize` request. |

With the config present, semantic ops (`definition`, `references`, `hover`, call hierarchy, …)
work for that language exactly like a built-in one. **Without it, the extension stays unsupported:
the engine resolves no server and the no-fallback contract applies** — semantic ops throw
`lspServerUnavailable` and the agent falls back to `localSearchCode` + `localGetFileContent`.
Both halves of this contract are asserted by the benchmark (`benchmark/lsp/check-lsp.mjs`,
"Custom LSP — bring-your-own server (Scala / metals)"), and verified **live against a real
server** by `benchmark/lsp/check-custom-lsp.mjs` (`yarn lsp:custom`) — which registers
`bash-language-server` (a language with no built-in spec) and runs real `documentSymbols` /
`references` / `hover` through it.

### Markup & docs: what's LSP vs minify

- **HTML / CSS / SCSS / LESS / JSON / YAML are LSP** — served by the bundled
  `vscode-*-language-server` / `yaml-language-server` (markup/data, offline-ready). They're
  not "code" languages but they do have real language servers.
- **Markdown / MDX are NOT LSP.** They are handled by the **minifier** using heading-section
  heuristics (ATX `#`/`##` and setext headings → `minify/strategies/markdown.rs`; `md`,
  `markdown`, `mdx` all map to the markdown strategy in `minify/config.rs`). There is no
  markdown language server in octocode — `documentSymbols` on a `.md` file uses the native
  heading-outline path, and structure/compression comes from the minifier, not a server.

## Full format support matrix

Per-extension capabilities across all four axes — minify strategy, structural AST,
signature outline, and LSP server — machine-generated from the shipped napi binary.
The LSP column here is the per-extension view of the [Supported language servers](#supported-language-servers)
table above.

<!-- BEGIN GENERATED: support-matrix (yarn matrix:check --write) — do not edit between these markers -->

_Generated by `yarn matrix:check --write` (benchmark/check-matrix.mjs) — every cell probed live against the shipped napi binary. Do not edit between the markers; run `yarn matrix:check` to re-verify._

> **Note:** The matrix queries the native layer (`config.rs`) only. Two servers injected at the TS layer (`config.ts`) are not reflected: **bash-language-server** for `.sh` (languageId `shellscript`) and **intelephense** for `.php` (the native spec emits `intelephense` as command — both bundled and active). Their LSP cells show `—` in the matrix below but the servers are bundled and resolve correctly (`npx octocode lsp-server status <file>` will show `resolved: bundled`).

**151 extensions** known to the engine — 61 with structural AST, 47 with a signature outline, 32 with an LSP server, 90 minify-only.

### Rich formats — AST + signature + LSP

Extensions with a wired tree-sitter grammar (and, where configured, a language server). The minify column is the configured strategy.

| Extension | Minify | Structural AST | Signature outline | LSP (server → language-id) |
|-----------|--------|:--------------:|-------------------|----------------------------|
| `.bash` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.c` | `conservative` | ✅ | ✅ tree-sitter | `clangd` → `c` |
| `.cc` | `conservative` | ✅ | ✅ tree-sitter | `clangd` → `cpp` |
| `.cjs` | `terser` | ✅ | ✅ tree-sitter | `typescript-language-server` → `javascript` |
| `.cpp` | `conservative` | ✅ | ✅ tree-sitter | `clangd` → `cpp` |
| `.cs` | `conservative` | ✅ | ✅ tree-sitter | `csharp-ls` → `csharp` |
| `.css` | `aggressive` | ✅ | — | `vscode-css-language-server` → `css` |
| `.cts` | `conservative` | ✅ | ✅ tree-sitter | `typescript-language-server` → `typescript` |
| `.cxx` | `conservative` | ✅ | ✅ tree-sitter | `clangd` → `cpp` |
| `.erl` | `aggressive` | ✅ | ✅ tree-sitter | — |
| `.ex` | `aggressive` | ✅ | ✅ tree-sitter | — |
| `.exs` | `aggressive` | ✅ | ✅ tree-sitter | — |
| `.gemspec` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.go` | `conservative` | ✅ | ✅ tree-sitter | `gopls` → `go` |
| `.h` | `conservative` | ✅ | ✅ tree-sitter | `clangd` → `c` |
| `.hcl` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.hh` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.hpp` | `conservative` | ✅ | ✅ tree-sitter | `clangd` → `cpp` |
| `.hrl` | `aggressive` | ✅ | ✅ tree-sitter | — |
| `.htm` | `aggressive` | ✅ | — | `vscode-html-language-server` → `html` |
| `.html` | `aggressive` | ✅ | — | `vscode-html-language-server` → `html` |
| `.hxx` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.java` | `conservative` | ✅ | ✅ tree-sitter | `jdtls` → `java` |
| `.jl` | `conservative` | ✅ | — | — |
| `.js` | `terser` | ✅ | ✅ tree-sitter | `typescript-language-server` → `javascript` |
| `.json` | `json` | ✅ | — | `vscode-json-language-server` → `json` |
| `.jsonc` | `json` | ✅ | — | `vscode-json-language-server` → `json` |
| `.jsx` | `terser` | ✅ | ✅ tree-sitter | `typescript-language-server` → `javascriptreact` |
| `.kt` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.kts` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.less` | `aggressive` | ✅ | — | `vscode-css-language-server` → `less` |
| `.lua` | `aggressive` | ✅ | ✅ tree-sitter | — |
| `.mjs` | `terser` | ✅ | ✅ tree-sitter | `typescript-language-server` → `javascript` |
| `.ml` | `conservative` | ✅ | — | — |
| `.mli` | `conservative` | ✅ | — | — |
| `.mts` | `conservative` | ✅ | ✅ tree-sitter | `typescript-language-server` → `typescript` |
| `.php` | `conservative` | ✅ | ✅ tree-sitter | `intelephense` → `php` |
| `.proto` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.py` | `conservative` | ✅ | ✅ tree-sitter | `pylsp` → `python` |
| `.pyi` | `conservative` | ✅ | ✅ tree-sitter | `pylsp` → `python` |
| `.r` | `aggressive` | ✅ | ✅ tree-sitter | — |
| `.rake` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.rb` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.rs` | `conservative` | ✅ | ✅ tree-sitter | `rust-analyzer` → `rust` |
| `.ru` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.sbt` | — | ✅ | ✅ tree-sitter | — |
| `.sc` | — | ✅ | ✅ tree-sitter | — |
| `.scala` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.scss` | `aggressive` | ✅ | — | `vscode-css-language-server` → `scss` |
| `.sh` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.sql` | `conservative` | ✅ | — | `sqls` → `sql` |
| `.swift` | `conservative` | ✅ | ✅ tree-sitter | `sourcekit-lsp` → `swift` |
| `.tf` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.tfvars` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.toml` | `conservative` | ✅ | — | — |
| `.ts` | `conservative` | ✅ | ✅ tree-sitter | `typescript-language-server` → `typescript` |
| `.tsx` | `conservative` | ✅ | ✅ tree-sitter | `typescript-language-server` → `typescriptreact` |
| `.yaml` | `conservative` | ✅ | — | `yaml-language-server` → `yaml` |
| `.yml` | `conservative` | ✅ | — | `yaml-language-server` → `yaml` |
| `.zig` | `conservative` | ✅ | ✅ tree-sitter | — |
| `.zsh` | `conservative` | ✅ | ✅ tree-sitter | — |

Notes:
- **Signature outline is tree-sitter only** — markup/style/config grammars (HTML/CSS/SCSS/LESS/Scala/JSON/YAML/TOML) parse for structural `rule` queries but have no function body, so no skeleton. There is **no** regex/heuristic fallback.
- **`.jsx`** resolves the LSP server as `javascriptreact` (to enable JSX) even though its tree-sitter grammar registry id is `javascript` (shared JS grammar). `.tsx` has its own grammar, so both ids are `typescriptreact`.
- **`.hh` / `.hxx`** have the C++ grammar + signatures but **no clangd server config** (only `.cpp/.cc/.cxx/.hpp` are mapped).
- **C/C++**: structural `rule` queries (e.g. `kind: call_expression`) work fully; a bare call-shaped `pattern` can hit tree-sitter's declaration-vs-call ambiguity — prefer a `rule` with `kind`. JS/TS also have a native (oxc) symbol/in-file-reference path that needs **no server installed**.

### Minify-only formats

Native comment/whitespace stripping; no AST/LSP. (90 extensions, grouped by strategy.)

**`aggressive`** (18): `clj` `cljs` `ejs` `erb` `handlebars` `hbs` `jinja` `jinja2` `mustache` `pl` `pm` `svelte` `svg` `twig` `vue` `xml` `xsl` `xslt`

**`conservative`** (66): `adb` `ads` `asm` `awk` `bzl` `cfg` `cmake` `coffee` `conf` `config` `csv` `dart` `dockerignore` `elm` `env` `f` `f03` `f08` `f90` `f95` `fish` `for` `fs` `fsx` `gitignore` `gql` `gradle` `graphql` `groovy` `haml` `hs` `ini` `jade` `kotlin` `lhs` `lisp` `lsp` `mm` `nasm` `nim` `nix` `pas` `perl` `plsql` `pp` `properties` `ps1` `psd1` `psm1` `pug` `rkt` `rst` `rust` `sass` `scm` `slim` `star` `styl` `tsql` `v` `vb` `vbs` `vhd` `vhdl` `wast` `wat`

**`general`** (2): `log` `txt`

**`json`** (1): `json5`

**`markdown`** (3): `markdown` `md` `mdx`

### Verify

```bash
yarn matrix:check     # this matrix, live
yarn ast:check        # structural search + signatures on real samples
yarn lsp:check        # language-id + server resolution + native semantics
yarn lsp:live         # spawn a real server, exercise every LSP operation type
yarn minify:check     # minifier over every configured format
yarn benchmark        # all of the above
```

<!-- END GENERATED: support-matrix -->

## Lifecycle — pool, cold start, indexing

- **Pool** (`lspClientPool.ts`): one warm `LSPClient` per (server × workspace), 60s idle timeout (`OCTOCODE_LSP_POOL_IDLE_MS`). A long-lived MCP session reuses warm servers across tool calls; one-shot CLI invocations don't share a pool.
- **Cold start / indexing**: a server reads the project and builds its model before answering correctly. Costs vary — typescript-language-server <1s, gopls 3–15s, rust-analyzer 5–60s (multiple `$/progress` waves), jdtls 30–120s.
- **Readiness** (`manager.ts` + `json_rpc.rs`): for servers that emit `$/progress` (go, rust, java, csharp, swift) the pool factory calls `waitForReady` with a per-language cap before the first query; servers without `$/progress` (TS/JS, Python, clangd, data formats) skip the wait to avoid a fixed 2s settle penalty.
- **Spawn gate**: every resolved command passes `validateLSPServerPath` (rejects shell wrappers / nonexistent / non-executable) in `LSPClient.start()` before the process is spawned.
- **Discovery caching** (`serverDiscovery.ts`): ecosystem-dir lookup results are memoised per `(command, workspaceRoot)` for the process lifetime. Ecosystem dirs are pre-filtered to existing ones once, cutting stat calls from ~15-per-server to ~5. Call `clearDiscoveryCache()` (or restart) after installing a server mid-session.

## Open / future (non-blocking)

Progress streaming to the caller (`lsp.indexingStatus`), an opt-in `waitForIndexingMs`, tree-sitter symbol extraction for compiled-language `documentSymbols`, and a SCIP precomputed index — all deferred; none change the no-fallback contract above.

> **Known issue — cold `references` under-reports.** On a one-shot CLI invocation,
> `references` (and other project-wide ops) can return incomplete results
> *labelled* `complete=true`, because `typescript-language-server` emits no
> `$/progress` and the project isn't indexed yet when queried. This is the one
> behaviour that can make an agent wrong rather than just slow — analysis and the
> proposed fix (honest `complete=false` + cross-check hint, with an opt-in
> `waitForIndexingMs`) are in
> [`LSP_REFERENCES_INDEXING_RFC.md`](https://github.com/bgauryy/octocode/blob/main/docs/context/LSP_REFERENCES_INDEXING_RFC.md). Meanwhile,
> prefer `--op callers` or `localSearchCode` to confirm "who uses this".
