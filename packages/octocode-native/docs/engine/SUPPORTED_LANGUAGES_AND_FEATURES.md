# Supported languages and features

Reference, not a tutorial. Regenerate the grammar list from the shipped CLI if this ever looks stale:

```bash
npx octocode schema | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>{
for (const g of JSON.parse(d).grammarCapabilities) console.log(g.language, g.extensions.join(' '), g.signatureOutline ? 'signatures' : '');})"
```

Minifier routing is internal. Test `localFetch` views with representative file
paths instead of depending on the strategy table.

## Structural (AST) search — `astSearch operation:"match"`

Tree-sitter-backed. Two query forms: `pattern` (code-shaped, `$X`/`$$$ARGS` metavars) and `rule` (YAML, `kind`/`has`/`inside`/`all`/`any`/`not`). YAML is the rule-document format; it does not imply YAML source parsing. A `rule: kind: NODE_KIND` query bypasses pattern-fragment parsing and dispatches directly to the registered grammar. Direct and nested rule patterns share the same grammar-checked fragment context.

| Language | Extensions | Native AST match/tree/symbols/rewrite | `astTopology` file links | Built-in LSP route |
|---|---|---|---|---|
| C | `c` `h` | Yes; `.h` defaults to C; `language:"cpp"` selects C++ for match and single-file tree/symbols; `languageGlobs` selects it for directory symbols/topology | Quoted relative includes | `clangd` |
| C++ | `cc` `cpp` `cxx` `hh` `hpp` `hxx` | Yes | Quoted relative includes | `clangd` |
| CUDA *(optional grammar)* | `cu` `cuh` | Only with `tree-sitter-cuda`; absent in the default build | Quoted relative includes only when grammar enabled | `clangd` even without native grammar |
| Assembly | `asm` `assembly` `s` | Yes; symbols are labels/directives | No cross-file links | Custom server only |
| C# | `cs` | Yes | No cross-file links | `csharp-ls` |
| Go | `go` | Yes | No cross-file links | `gopls` |
| Java | `java` | Yes | No cross-file links | `jdtls` |
| Python | `py` `pyi` | Yes | Bounded absolute/relative modules | `pylsp` |
| Rust | `rs` | Yes | Modules; optional Cargo metadata | `rust-analyzer` |
| Scala | `sc` `sbt` `scala` | Yes | No cross-file links | `metals` |
| JavaScript | `js` `jsx` `mjs` `cjs` | Yes | ESM and binding-safe CommonJS | `typescript-language-server` |
| TypeScript | `ts` `tsx` `mts` `cts` | Yes | ESM and binding-safe CommonJS | `typescript-language-server` |

`structureSearch` (`tree`/`files`) is language-agnostic. `astRewrite` and `astTopology` are beta-gated; the matrix lists their capabilities when enabled. Native AST availability does not install or guarantee an LSP server or every LSP operation. Tree-sitter owns JS/TS structural matching while OXC supplies richer JS/TS facts. C++ function patterns repair a narrow C++11 initializer-list ambiguity; C# member patterns use a synthetic class wrapper; Java method-call patterns receive statement context. Uppercase `.S` normalizes to `.s`.

### Which operation to use

| Agent needs | Operation | Evidence and limit |
|---|---|---|
| Paths or file metadata | `structureSearch files` / `tree` | Filesystem result; no grammar required |
| Syntax pattern or node kind | `astSearch match` | Native grammar; provide `language` for a directory, or let a file extension select it |
| Parsed node tree | `astSearch syntaxTree` | Syntax only; does not resolve symbol identity |
| Declaration outline | `astSearch symbols` | Native declarations; a row's `name`+`line` is `lspSearch` `symbolName`+`lineHint` as-is; read source for bodies and exact claims |
| Structural edit | `astRewrite` | Beta-gated preview and hash-guarded apply |
| File links, cycles, reachability | `astTopology` | Syntax-derived candidate graph; confirm delete claims with LSP references/callers |
| Definition, references, types, hover, implementations | `lspSearch definition/references/typeDefinition/hover/implementation` | Requires an installed server and its advertised capability; use an observed symbol anchor |
| Call hierarchy or inheritance | `lspSearch callers/callees/supertypes/subtypes` | Requires the corresponding server capability; syntax graph edges alone are not semantic proof |
| File outline | `lspSearch documentSymbols` | Usually server-backed; JS/TS has a syntactic native fallback identified by `lsp.source` |
| Workspace name lookup | `lspSearch workspaceSymbol` | Supply `uri` in a mixed-language workspace to select the server |
| Compiler or language-server findings | `lspSearch diagnostic` | Requires a server; reports its diagnostics rather than native grammar support |

### Search and rewrite matchers

`astSearch match` runs Octocode's own matcher (`crates/engine/src/structural/octo/`); `astRewrite` runs embedded ast-grep. Both read ast-grep pattern syntax over the same grammar registry, and `crates/engine/src/structural/parity_tests.rs` pins where they agree. Shared: single and multi metavariables (separators included in `$$$` captures), repeated captures, an empty `$$$` beside punctuation (`f($$$A, x)` matches `f(x)`), candidate trivia (comments, trailing commas), `kind`, `regex`, `has`/`inside` (direct parent/children by default, any depth with `stopBy: end`), `all`/`any`/`not`, and UTF-16 columns. Differences:

- `astSearch` rule YAML accepts only `kind`, `pattern`, `regex`, `has`, `inside`, `all`, `any`, `not`, and `stopBy: end`; `constraints`, `follows`, `precedes`, and other ast-grep keys are rejected with the supported list.
- A statement pattern ending in `;` matches the expression anywhere in `astSearch` (span without `;`); ast-grep selects only that statement, `;` included. Omit the `;` when a search result set feeds a rewrite.
- `$K: $V` selects object pairs in `astSearch`; ast-grep parses it as a labeled statement.
- A bare C call pattern (`foo($X)`) gets statement context in `astSearch`; ast-grep parses it as a declaration and selects nothing. Use `foo($X);` or a statement-level pattern in C rewrites.
- A pattern with several top-level nodes (`a(); b()`) is rejected by both.
- Rewrite captures can include ast-grep's internal `secondary` node from relational rules.

A search result never authorizes an apply: only an `astRewrite` preview's selection, snapshot, and hashes do.

The default release build registers exactly **28 extensions across 11 language families**. CUDA (`.cu`/`.cuh`) is an optional grammar (`tree-sitter-cuda`) excluded from the default build to save ~6.8 MiB of binary size; its native capabilities appear only in builds that re-enable the feature, though `.cu`/`.cuh` still route to `clangd` for LSP. Structural search/rewrite, signatures, graph facts, syntax inspection, and LSP grammar adapters derive from the single registry in `crates/engine/src/signatures/languages.rs`. Exact expected-set assertions live in `crates/engine/src/signatures/languages_tests.rs`; every retained grammar also parses and searches a representative fixture. Built-in semantic-server routing is intentionally narrower because generic Assembly has no truthful default server.

### CUDA opt-in and cost

`tree-sitter-cuda` is already declared and locked. For a one-off runtime build, enable the dependency feature with `--features octocode-engine/tree-sitter-cuda`. To ship it in every build, add `tree-sitter-cuda` to the engine's `portable-default` feature, which the CLI and runtime consume. Then update the fixed default extension expectation in `crates/engine/src/signatures/languages_tests.rs`, the documented counts, and build/test the six platform packages. No new grammar dependency or LSP route is required.

The [same-source Darwin ARM64 release ablation](../DEPENDENCY_AUDIT.md#footprint-interpretation) measured **+7,116,704 bytes (+6.787 MiB, +25.53%)** in the stripped engine addon with CUDA enabled. That measures one addon, not the total platform package, compressed download, CLI binary, or runtime addon; those need separate release measurements before changing the default. The optional engine and runtime grammar tests pass with CUDA enabled. Text search, ordinary reads, conservative minification, and the `clangd` LSP route already work for `.cu`/`.cuh` without this parser feature.

The lockfile contains no other unregistered Tree-sitter language crate. OXC covers JS/TS, while JSON/YAML parsers and the broader minifier table do not supply the source ranges and grammar queries required by AST match, rewrite, symbols, and topology. C++ `.h` files expose a separate ambiguity: `.h` selects C by default. `astSearch match` with `language:"cpp"` parses matching `.h` files as C++ for either a file or directory; tree and symbols accept that override for a single file. `astRewrite` uses `language` for its parser and filters directory scans to the selected language's extensions, including `.h` for C++. For directory `symbols` and `astTopology`, pass `languageGlobs:{"cpp":["include/**/*.h"]}`. Globs are relative to the scan root, take precedence over the extension parser, and are included in continuation queries; conflicting parser matches are reported as skipped files. This AST override does not alter clangd's compile-command handling. For C++ header LSP analysis, provide `compile_commands.json` or a path-scoped `.clangd` fragment such as `If: { PathMatch: include/.*\.h }` with `CompileFlags: { Add: [-xc++] }`. `languageGlobs` uses glob syntax; `.clangd` `PathMatch` uses a regular expression.

## Signature extraction / graph facts — `minify:"symbols"`, `astTopology`

`localFetch minify:"symbols"` provides skeleton outlines. All supported code languages, including JS/TS, use Tree-sitter body queries for signature skeletons. OXC provides JS/TS graph facts, native document symbols and in-file references, and minification. Graph facts are syntax-derived and vary by language; signature capability does not establish complete declaration or call extraction.

Cross-file graph linking covers JavaScript/TypeScript ESM and binding-safe CommonJS, Rust modules, bounded Python absolute and relative imports, and quoted relative C/C++/CUDA includes. Explicit relative `package.json` imports become bounded metadata leaves. CommonJS links require an unshadowed literal `require`, `module.require`, or `createRequire(import.meta.url)` binding; dynamic, shadowed, reassigned, and otherwise ambiguous loaders remain coverage diagnostics. Python wildcards, ambiguous package attributes, and ambiguous stub layouts remain diagnostics. C/C++/CUDA system and macro includes are not linked. Other languages report unsupported cross-file linking rather than producing heuristic edges.

Supported: the same exact 28 extensions in the structural table. Every registered grammar has a real body query; use the capability APIs for builds with optional Assembly, C++, C#, CUDA, or Scala features turned off. Assembly graph facts expose labels as declarations but deliberately do not claim dialect-neutral calls, containment, exports, or cross-file links.

Kotlin (`kt`/`kts`), PHP (`php`), CSS (`css`), HTML (`html`/`htm`), JSON (`json`/`jsonc`), TOML, Lua, Zig, Ruby, SCSS, SQL, Swift, YAML, Elixir, HCL/Terraform, Protobuf, Shell, Less, OCaml, Julia, R, Erlang, Vue, Svelte, Astro, Dart, and other non-target languages have no native source grammar. Grammar-dependent operations return a typed unsupported result instead of selecting another parser.

Text search, ordinary reads, GitHub/history operations, artifact lookup, file recognition, generic best-effort minification, and trusted custom LSP routes remain language-agnostic. Packagist/Composer and Maven artifact search are unaffected by removal of PHP and Kotlin source parsing.

## Minification — file reads and search fragments

Minification support is separate from parser and LSP support. The default
configuration contains **152 extension entries**, plus 15
filename overrides. Many entries use comment and whitespace processing without
a syntax parser. Scala `.scala`, `.sc`, and `.sbt` share one strategy.

| View | Processing | Research use |
|---|---|---|
| `none` | Skips minification; extraction, security redaction, and response formatting still apply | Source evidence, comments, type declarations, edits, and literal matches |
| `standard` | Uses language-dependent processing. JS/TS strips comments and tightens whitespace while preserving identifiers, type declarations, and statement lines; other strategies compact JSON, markup, CSS, Markdown, or comments and whitespace | Orientation; use `none` for exact text, comments, and formatting |
| `symbols` | Extracts an outline for the registered first-class extensions; Markdown has a heading fallback. Unsupported or unavailable outlines fall back to `standard` | Declaration locations and source-line anchors; follow with an exact read for bodies |

For file reads, `fullContent:true` defaults to `none`. Local line ranges also
default to `none`; GitHub line ranges and other ordinary reads default to
`standard`. Both matching readers force `none`. Both readers paginate outlines
and reject outline queries combined with matching or line-range selectors.
Explicit character windows apply even with `fullContent:true`.

GitHub code-search fragments are returned unminified. Treat snippets as discovery evidence and read the source with `minify:"none"`
before quoting or checking identifier usage.

Native regression coverage exercises all 152 configured extensions in the
content view, all 15 filename overrides, and embedded script views.
Public file-read tests exercise retained and removed-language fixtures, transformed views, and continuations. Minification coverage is intentionally broader than the first-class parser set and is representative rather than exhaustive language conformance.

History has a separate contract: PR details accept `none` or `standard` for
body, comments, reviews, and patches. Diff compaction is language-independent
and preserves changed source lines. Issue, commit, and compare details return
exact selected content and do not accept a file-view `minify` option.

Native minification can return the original input when the result is not
smaller, processing fails, or input exceeds its 1 MiB guard. A selected mode
does not establish which transformations ran. See the
[tool reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) for extraction and pagination.

## LSP — `lspSearch`

Built-in LSP routing covers 11 language families and 27 extensions. CUDA `.cu`/`.cuh` files route to `clangd`; Scala routes `.scala`, `.sc`, and `.sbt` to Metals. Assembly remains grammar-backed for syntax anchoring but requires a trusted custom language-server configuration because `clangd` does not provide a truthful generic Assembly semantic route. `typescript-language-server --stdio` remains the stable JS/TS default; `tsgo` is available only through an explicit override until parity evidence exists. Python currently defaults to `pylsp`; BasedPyright/Pyright preference requires a separately committed executable and behavior matrix.

| Source | Languages | What happens |
|---|---|---|
| **Workspace, ecosystem, or PATH** | The 11 built-in language families | Resolves an installed known command; the engine npm package does not bundle language servers |
| **Managed cache** | Rust (`rust-analyzer`), C/C++/CUDA (`clangd`) | Uses assets explicitly installed by `npx octocode lsp-server install` after HTTPS and SHA-256 verification |
| **Custom configuration** | Any extension, including removed first-class routes | Registers an extension, command, arguments, and language ID; project configuration requires explicit trust |

`documentSymbols`, `definition`, `references`, `callers`, `callees`,
`hover`, `typeDefinition`, `implementation`,
`workspaceSymbol`, `supertypes`, `subtypes`, and `diagnostic` are public
operations. A running server that lacks an operation returns a typed `empty`
payload such as `unsupportedOperation`. A missing server returns the typed
`serverUnavailable` error instead of a syntax-derived semantic answer.

`documentSymbols` is the outline exception: JS/TS native outlines and Markdown
headings run without starting or checking a server. Their `lsp.source` identifies
the syntax source and their evidence is `syntactic`, not `semantic`. Other
document outlines require a running server with `documentSymbolProvider`.
Initialization and request failures remain errors; they are not evidence that a
server lacks a capability. Consumer-file warmup runs only for supported relation
operations. JSX opens with the protocol language ID `javascriptreact`, even
though its syntax parser uses the JavaScript grammar.

Pagination continuations carry a result snapshot. Execute the returned `next`
query unchanged. If the result set or query changes between pages, the tool
returns `paginationChanged` and a restart query, without stale page rows.

Engine and native Rust tests cover route resolution, uppercase extensions,
unsupported file types, missing servers, startup failures, capabilities,
readiness, cancellation, and pagination snapshots. Deterministic tests do not
establish that an external server is installed or implements every operation.

For `workspaceSymbol`, provide `uri` as a language anchor in mixed-language
workspaces. The request accepts `workspaceRoot` without `uri`, but server
selection can otherwise depend on the first file returned by anchor discovery.

## Verify the shipped build

Use the compiled capability APIs for exact extension lists, then exercise the
public CLI or MCP tool path. Do not infer native grammar support from an LSP
route.

```bash
npx octocode schema | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>{
for (const g of JSON.parse(d).grammarCapabilities) console.log(g.language, g.extensions.join(' '));})"

npx octocode astSearch \
    '{"queries":[{"operation":"match","path":"/ABS/REPO","pattern":"$$$","language":"typescript","reasoning":"Probe grammar support."}]}'

npx octocode lspSearch \
    '{"uri":"/ABS/REPO/src/file.ts","operation":"documentSymbols","reasoning":"Probe LSP support."}'
```

Run the package tests with these commands:

```bash
yarn workspace @octocodeai/octocode-native test:rust
yarn workspace @octocodeai/octocode-native test:node
```

## `localSearch` lexical search

| Feature | Values |
|---|---|
| `regex` | `rust` (default) · `literal` · `pcre2` (lookaround/backreferences) |
| `caseMode` | `smart` · `sensitive` · `insensitive` |
| `wholeWord`, `invertMatch` | boolean |
| `multiline` | `off` · `on` · `dotall` (`.` spans newlines) |
| `resultView` | `paginated` · `detailed` · `content` · `files` · `filesWithout` · `countLines` · `countMatches` · `matchOnly` |
| `unique` | `off` · `list` · `count` (requires `resultView:"matchOnly"`) |
| `sort` / `reverse` | `relevance` · `matchCount` · `path` · `modified` · `accessed` · `created`, all reversible |
| `include` / `exclude` | glob arrays |
| `maxDepth`, `contextLines`, `matchPage`, `matchPageSize` | bounds/pagination |

Read the live `localSearch` schema before scripting queries. It is lexical only;
use `structureSearch` for directory outlines and file metadata, `astSearch` for structural, syntax-tree, and symbol operations, and `astTopology` for file-graph analysis.
