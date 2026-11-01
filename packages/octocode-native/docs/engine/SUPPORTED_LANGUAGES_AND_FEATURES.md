# Supported languages and features

Which languages each native capability covers. Tool fields and behavior are in the [tools reference](../../../../docs/OCTOCODE_TOOLS.md); which evidence to choose is in [Text, AST, graph, and LSP](../../../../docs/OCTOCODE_TOOLS.md#text-ast-graph-and-lsp-choose-the-evidence-you-need). The shipped build prints its grammar inventory:

```bash
npx octocode schema --view full | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>console.log(JSON.stringify(JSON.parse(d).grammarCapabilities,null,2)))"
```

## Language matrix

The default build registers **13 grammar entries covering 30 extensions in 12 language families**; TypeScript and TSX are separate entries. The registry lives in `crates/engine/src/signatures/languages/registry.rs`, with membership checked in `signatures/languages/tests/registry.rs`. Structural match, syntax trees, symbols, signatures, graph facts, and LSP grammar adapters derive from it. Built-in language-server routes are a separate table (`crates/engine/src/lsp/config/catalog.rs`) covering **27 extensions**: Assembly has no route.

| Language | Extensions | Native AST search | Cross-file import linking | Built-in LSP route |
|---|---|---|---|---|
| C | `c` `h` | Yes; `.h` defaults to C (see [C++ headers](#c-headers)) | Quoted relative includes | `clangd` |
| C++ | `cc` `cpp` `cxx` `hh` `hpp` `hxx` | Yes | Quoted relative includes | `clangd` |
| CUDA | `cu` `cuh` | Yes | Quoted relative includes | `clangd` |
| Assembly | `asm` `assembly` `s` | Yes; symbols are labels and directives | None | Custom server only |
| C# | `cs` | Yes | None | `csharp-ls` |
| Go | `go` | Yes | Go packages under a readable `go.mod` | `gopls` |
| Java | `java` | Yes | Imports and same-package classes | `jdtls` |
| Python | `py` `pyi` | Yes | Bounded absolute and relative modules | `basedpyright-langserver`, else `pyright-langserver`, else `pylsp` |
| Rust | `rs` | Yes | Modules; optional Cargo metadata | `rust-analyzer` |
| Scala | `sc` `sbt` `scala` | Yes | None | `metals` |
| JavaScript | `js` `jsx` `mjs` `cjs` | Yes | ESM and binding-safe CommonJS | `typescript-language-server` |
| TypeScript | `ts` `tsx` `mts` `cts` | Yes | ESM and binding-safe CommonJS | `tsgo`, else `typescript-language-server` |

Cross-file graph links feed the CLI's `octocode graph`; the matrix lists engine support. A grammar does not install or guarantee a language server. Tree-sitter owns structural matching for every language, JS/TS included; OXC adds JS/TS symbols, in-file references, calls, and graph facts. Uppercase `.S` normalizes to `.s`.

Languages with no native grammar (Kotlin, PHP, CSS, HTML, JSON, TOML, YAML, Ruby, Swift, Shell, SQL, Lua, Zig, Vue, Svelte, Dart, and others) cannot be selected as native parsers. Grammar-dependent operations report unsupported input where an explicit parser or file is required; directory graph discovery skips extensions outside the registry. A successful graph with `filesScanned:0` does not prove those files contain no declarations. Text search, reads, GitHub/history and package lookup do not require a native grammar; minification and custom LSP routes have their own support boundaries.

### C++ headers

`.h` selects C. `astSearch` with `operation:"match"` and `parser:"cpp"` parses selected `.h` files as C++; `syntaxTree` and `symbols` accept that override for a single file. For directory `symbols` or `graphBuild`, pass `parserGlobs:{"cpp":["include/**/*.h"]}`: globs are relative to the scan root, win over the extension, carry into continuations, and report conflicting parser matches as skipped files. The override does not change clangd: for C++ header LSP, provide `compile_commands.json` or a `.clangd` fragment such as `If: { PathMatch: include/.*\.h }` with `CompileFlags: { Add: [-xc++] }` (`PathMatch` is a regular expression, not a glob).

### Grammar features

Every grammar above ships in the default build. Assembly, C++, C#, CUDA and Scala are optional engine features, all enabled by `portable-default`; a build without one returns the typed unsupported result for its extensions, and `signatures/languages/tests/registry.rs` derives the expected set from the enabled features. CUDA is the largest grammar, about 6.8 MiB per artifact ([measurement](../DEPENDENCY_AUDIT.md#footprint-interpretation)).

The inventory reports structural search, syntax trees, signature outlines, declaration facts, graph fact families and region adapters independently. An installed grammar is not evidence that every operation is available. Unknown languages report `languageUnsupported`; a registered provider without the requested operation reports `capabilityUnavailable`. The shared registry can admit a facts provider without a Tree-sitter grammar or rendered outline. This is an internal extension boundary, not support for installing arbitrary adapters at runtime; TOML remains unshipped.

`sourceDecoding` describes source admission separately from grammar support. Raw-file offsets and analyzer offsets are connected by [source correspondence](SOURCE_CORRESPONDENCE.md); region-adapter availability does not imply semantic binding or value-flow analysis.

## Structural matchers

`astSearch match` uses Octocode's matcher (`crates/engine/src/structural/octo/`) over the shared grammar registry. It supports single and multi metavariables, repeated captures, comments and trailing commas in candidates, `kind`, `regex`, `has`/`inside` (direct by default, any depth with `stopBy: end`), and `all`/`any`/`not`. Unsupported rule keys are rejected with the supported list.

A pattern must contain one complete node. Several top-level nodes (`a(); b()`) are rejected. C calls and Java method calls receive statement context; C# member patterns receive class context; C++ function patterns handle initializer-list ambiguity. Results identify syntax, not symbol identity.

## Signatures and graph facts

`localFetch view:"symbols"` outlines every registered language with Tree-sitter body queries; Markdown has a heading fallback. Graph facts are syntax-derived and vary by language; an outline does not establish complete declaration or call extraction. Assembly exposes labels as declarations and claims no calls, containment, exports, or cross-file links.

All 13 entries can contribute declarations to a graph. The 12 entries other than Assembly declare call, import, export and direct argument-reference extraction. Heritage extraction is declared for TypeScript, TSX, JavaScript, Python, Rust, Go (embedding), Java, C++, CUDA and C#. These flags identify implemented fact families, not complete support for every construct. A `passes` relation observes an argument reference; it does not establish callback invocation or value flow. Inspect `stats.languages`, call coverage, `argumentCoverage`, diagnostics and unresolved occurrences before making absence claims. Graph tools run in the CLI and are exposed in MCP when `OCTOCODE_BETA=true`; both surfaces use this same grammar registry.

Cross-file import links cover JS/TS ESM and binding-safe CommonJS, Rust modules, bounded Python absolute and relative imports, Go packages, Java imports/same-package classes, and quoted relative C/C++/CUDA includes. Explicit relative `package.json` imports become bounded metadata leaves. CommonJS links need an unshadowed literal `require`, `module.require`, or `createRequire(import.meta.url)` binding; dynamic, shadowed, or reassigned loaders, ambiguous package attributes and stub layouts stay coverage diagnostics. C/C++ system and macro includes are not linked. Assembly, C# and Scala report unsupported import linking. In-file facts and syntax candidates can still exist for C# and Scala; an import fact does not prove a resolved dependency.

## Minification — file reads and search fragments

Minification is independent of grammar and LSP support. The engine configures **152 extensions** plus filename overrides; most entries strip comments and whitespace without a parser, JSON, Markdown, markup, and CSS have their own strategies, and Scala `.scala`, `.sc`, and `.sbt` share one. YAML uses the verbatim strategy: generic comment and whitespace cleanup cannot safely distinguish scalar payloads. Output is also the original input when the result is not smaller, processing fails, or the input exceeds 1 MiB, so a requested view does not prove which transformation ran. `crates/engine/src/minify/coverage_tests.rs` exercises every configured extension and filename override; `literal_tests.rs` covers multiline literal bytes and YAML scalar preservation. View semantics and defaults per tool: [tools reference](../../../../docs/OCTOCODE_TOOLS.md#localfetch).

## Language servers

Routing order, trust, provisioning, pooling, and memory caps are in [LSP_SERVER_LIFECYCLE.md](LSP_SERVER_LIFECYCLE.md). Managed installs (`npx octocode lsp-server install`) cover `rust-analyzer` and `clangd`; every other built-in route uses an installed executable. A custom `lsp-servers.json` entry adds or replaces a route for any extension. Every `lspSearch` operation needs a running server with the matching capability. Use `localFetch view:"symbols"` for syntax outlines or Markdown headings without a server; Markdown LSP needs a custom server route.

## Verify a build

Read the grammar inventory (above), then exercise the public path. Don't infer grammar support from an LSP route.

```bash
npx octocode astSearch '{"queries":[{"operation":"symbols","path":"src/file.ts"}]}'
npx octocode lspSearch '{"queries":[{"operation":"documentSymbols","path":"src/file.ts"}]}'
yarn workspace @octocodeai/octocode-native test:rust
yarn workspace @octocodeai/octocode-native test:node
```

Deterministic tests cover route resolution, uppercase extensions, unsupported file types, missing servers, startup failures, capabilities, readiness, cancellation, and pagination snapshots; they do not prove that an external server is installed or implements every operation.
