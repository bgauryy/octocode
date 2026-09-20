# Supported languages and features

Reference, not a tutorial. Regenerate the extension lists from the engine itself if this ever looks stale:

```bash
node -e "const n=require('@octocodeai/octocode-native/engine');
console.log('structural', n.getSupportedStructuralExtensions().sort());
console.log('signatures', n.getSupportedSignatureExtensions().sort());
console.log('jsts', n.getSupportedJsTsExtensions().sort());"
```

Minifier routing is internal. Test `minifyContent()` or
`applyContentViewMinification()` with representative file paths instead of
depending on the strategy table.

## Structural (AST) search — `astSearch operation:"match"`

Tree-sitter-backed. Two query forms: `pattern` (code-shaped, `$X`/`$$$ARGS` metavars) and `rule` (YAML, `kind`/`has`/`inside`/`all`/`any`/`not`). YAML is the rule-document format; it does not imply YAML source parsing. A `rule: kind: NODE_KIND` query bypasses pattern-fragment parsing and dispatches directly to the registered grammar. Direct and nested rule patterns share the same grammar-checked fragment context.

| Language | Extensions | Notes |
|---|---|---|
| C | `c` `h` | `.h` defaults to C; explicitly select C++ when project context requires it |
| C++ | `cc` `cpp` `cxx` `hh` `hpp` `hxx` | Function patterns repair a narrow C++11 initializer-list ambiguity only when the alternate parse is a function definition |
| C# | `cs` | Member patterns use a transparent synthetic wrapper class for grammar context |
| Go | `go` | |
| Java | `java` | Bare method-call patterns receive grammar-checked statement context |
| Python | `py` `pyi` | |
| Rust | `rs` | |
| Scala | `sc` `sbt` `scala` | |
| JavaScript | `js` `jsx` `mjs` `cjs` | Tree-sitter owns structural matching; OXC owns richer JS analysis |
| TypeScript | `ts` `tsx` `mts` `cts` | Tree-sitter owns structural matching; OXC owns richer TS analysis |

The default release build registers exactly **25 extensions across 10 language families**. Structural, signature, graph-fact, rewrite, and built-in LSP route tests share this boundary. Exact expected-set assertions live in `crates/engine/src/signatures/languages_tests.rs` and `tests/engine/ffi.test.ts`; every retained grammar also parses and searches a representative fixture.

## Signature extraction / graph facts — `minify:"symbols"`, `astSearch operation:"topology"`

`localFetch minify:"symbols"` provides skeleton outlines. All supported code languages, including JS/TS, use Tree-sitter body queries for signature skeletons. OXC provides JS/TS graph facts, native document symbols and in-file references, and minification. Graph facts are syntax-derived and vary by language; signature capability does not establish complete declaration or call extraction.

Cross-file graph linking covers JavaScript/TypeScript ESM and binding-safe CommonJS, Rust modules, bounded Python absolute and relative imports, and quoted relative C/C++ includes. Explicit relative `package.json` imports become bounded metadata leaves. CommonJS links require an unshadowed literal `require`, `module.require`, or `createRequire(import.meta.url)` binding; dynamic, shadowed, reassigned, and otherwise ambiguous loaders remain coverage diagnostics. Python wildcards, ambiguous package attributes, and ambiguous stub layouts remain diagnostics. C/C++ system and macro includes are not linked. Other languages report unsupported cross-file linking rather than producing heuristic edges.

Supported: the same exact 25 extensions in the structural table. Every registered grammar has a real body query; use the capability APIs for builds with optional C++, C#, or Scala features turned off.

Kotlin (`kt`/`kts`), PHP (`php`), CSS (`css`), HTML (`html`/`htm`), JSON (`json`/`jsonc`), TOML, Lua, Zig, Ruby, SCSS, SQL, Swift, YAML, Elixir, HCL/Terraform, Protobuf, Shell, Less, OCaml, Julia, R, Erlang, Vue, Svelte, Astro, Dart, and other non-target languages have no native source grammar. Grammar-dependent operations return a typed unsupported result instead of selecting another parser.

Text search, ordinary reads, GitHub/history operations, artifact lookup, file recognition, generic best-effort minification, and trusted custom LSP routes remain language-agnostic. Packagist/Composer and Maven artifact search are unaffected by removal of PHP and Kotlin source parsing.

## Minification — file reads and search fragments

Minification support is separate from parser and LSP support. The default
configuration contains **148 extension entries**, plus 15
filename overrides. Many entries use comment and whitespace processing without
a syntax parser. Scala `.scala`, `.sc`, and `.sbt` share one strategy.

| View | Processing | Research use |
|---|---|---|
| `none` | Skips minification; extraction, security redaction, and response formatting still apply | Source evidence, comments, type declarations, edits, and literal matches |
| `standard` | Uses language-dependent processing. JS/TS uses OXC compact code generation without optimization, mangling, or type-declaration removal; other strategies compact JSON, markup, CSS, Markdown, or comments and whitespace | Orientation; use `none` for exact text, comments, and formatting |
| `symbols` | Extracts an outline for the 25 first-class extensions; Markdown has a heading fallback. Unsupported or unavailable outlines fall back to `standard` | Declaration locations and source-line anchors; follow with an exact read for bodies |

For file reads, `fullContent:true` defaults to `none`. Local line ranges also
default to `none`; GitHub line ranges and other ordinary reads default to
`standard`. Both matching readers force `none`. Both readers paginate outlines
and reject outline queries combined with matching or line-range selectors.
Explicit character windows apply even with `fullContent:true`.

GitHub code-search fragments use the stronger full-content minifier, which can
inline or remove local bindings. If compression removes a provider match that
survived security redaction, the fragment falls back to its sanitized source.
Treat snippets as discovery evidence and read the source with `minify:"none"`
before quoting or checking identifier usage.

Native regression coverage exercises all 148 configured extensions in standard
and full minification, all 15 filename overrides, and embedded script views.
Public file-read and engine FFI tests exercise retained and removed-language fixtures, transformed views, and continuations. Minification coverage is intentionally broader than the first-class parser set and is representative rather than exhaustive language conformance.

History has a separate contract: PR details accept `none` or `standard` for
body, comments, reviews, and patches. Diff compaction is language-independent
and preserves changed source lines. Issue, commit, and compare details return
exact selected content and do not accept a file-view `minify` option.

Native minification can return the original input when the result is not
smaller, processing fails, or input exceeds its 1 MiB guard. A selected mode
does not establish which transformations ran. See the
[tool reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) for extraction and pagination.

## LSP — `lspSearch`

Built-in LSP routing covers the same ten first-class language families and 25 extensions. Scala routes `.scala`, `.sc`, and `.sbt` to Metals. `typescript-language-server --stdio` remains the stable JS/TS default; `tsgo` is available only through an explicit override until parity evidence exists. Python currently defaults to `pylsp`; BasedPyright/Pyright preference requires a separately committed executable and behavior matrix.

| Source | Languages | What happens |
|---|---|---|
| **Workspace, ecosystem, or PATH** | The ten built-in language families | Resolves an installed known command; the engine npm package does not bundle language servers |
| **Managed cache** | Rust (`rust-analyzer`), C/C++ (`clangd`) | Uses assets explicitly installed by `octocode lsp-server install` after HTTPS and SHA-256 verification |
| **Language-specific override** | Built-in routes | Uses the matching `OCTOCODE_*_SERVER_PATH` command after executable validation |
| **Custom configuration** | Any extension, including removed first-class routes | Registers an extension, command, arguments, and language ID; project configuration requires explicit trust |

`documentSymbols`, `definition`, `references`, `callers`, `callees`,
`callHierarchy`, `hover`, `typeDefinition`, `implementation`,
`workspaceSymbol`, `supertypes`, `subtypes`, and `diagnostic` are public
operations. A running server that lacks an operation returns a typed `empty`
payload such as `unsupportedOperation`. A missing server returns the typed
`lspServerUnavailable` error instead of a syntax-derived semantic answer.

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
node -e "const n=require('@octocodeai/octocode-native/engine'); \
console.log(n.getSupportedStructuralExtensions().sort()); \
console.log(n.getSupportedSignatureExtensions().sort())"

node packages/octocode/out/octocode.js astSearch \
    '{"operation":"match","path":"/ABS/REPO","pattern":"$$$","langType":"typescript","reasoning":"Probe grammar support."}'

node packages/octocode/out/octocode.js lspSearch \
    '{"uri":"/ABS/REPO/src/file.ts","operation":"documentSymbols","reasoning":"Probe LSP support."}'
```

Run the package tests with these commands:

```bash
yarn workspace @octocodeai/octocode-native test:rust
yarn workspace @octocodeai/octocode-native test:node
```

## `localSearch` lexical search (ripgrep-backed)

| Feature | Values |
|---|---|
| `regex` | `rust` (default) · `literal` · `pcre2` (lookaround/backreferences) |
| `caseMode` | `smart` · `sensitive` · `insensitive` |
| `wholeWord`, `invertMatch` | boolean |
| `multiline` | `off` · `on` · `dotall` (`.` spans newlines) |
| `resultView` | `paginated` · `discovery` · `detailed` · `content` · `files` · `filesWithout` · `countLines` · `countMatches` · `matchOnly` |
| `unique` | `off` · `list` · `count` (requires `resultView:"matchOnly"`) |
| `sort` / `reverse` | `relevance` · `matchCount` · `path` · `modified` · `accessed` · `created`, all reversible |
| `include` / `exclude` / `excludeDir` | glob arrays |
| `maxDepth`, `contextLines`, `matchWindow`, `matchPage`, `maxMatchesPerFile` | bounds/pagination |

Read the live `localSearch` schema before scripting queries. It is lexical only;
use `astSearch` for structural, file, tree, symbol, and topology operations.
