# Supported languages and features

Reference, not a tutorial. Regenerate the extension lists from the engine itself if this ever looks stale:

```bash
node -e "const n=require('./packages/octocode-engine/index.js');
console.log('structural', n.getSupportedStructuralExtensions().sort());
console.log('signatures', n.getSupportedSignatureExtensions().sort());
console.log('jsts', n.getSupportedJsTsExtensions().sort());
console.log('minify', Object.keys(n.getMINIFY_CONFIG().fileTypes).sort());"
```

## Structural (AST) search — `localSearch operation:"structural"`

Tree-sitter-backed. Two query forms: `pattern` (code-shaped, `$X`/`$$$ARGS` metavars) and `rule` (YAML, `kind`/`has`/`inside`/`all`/`any`/`not`). A `rule: kind: NODE_KIND` query bypasses pattern-fragment parsing and dispatches directly to the registered grammar. Use it when `pattern` parsing does not represent the intended code shape. Direct and nested rule patterns share the same grammar-checked fragment context: Java calls and CSS/SCSS declarations may omit a trailing semicolon without changing matches or capture ranges for complete patterns.

| Language | Extensions | Pattern evidence | Notes |
|---|---|---|---|
| C | `c` `h` | ✅ direct fixture | |
| C++ | `cc` `cpp` `cxx` `hh` `hpp` `hxx` | ✅ direct fixture | Function patterns with `$$$BODY` repair the narrow C++11 initializer-list ambiguity only when the alternate parse is a `function_definition`; ordinary initializer-list patterns keep their native parse |
| C# | `cs` | ✅ direct fixture | The matcher parses patterns inside a transparent synthetic wrapper class to provide member context |
| Go | `go` | ✅ matrix fixture | Representative patterns; not every syntax construct |
| Java | `java` | ✅ direct and composed-rule fixtures | Bare method-call patterns receive grammar-checked statement context; complete patterns keep their native parse |
| Kotlin | `kt` `kts` | ✅ matrix fixtures | Representative patterns; not every syntax construct |
| Lua | `lua` | ✅ direct fixture | |
| PHP | `php` | ⚠️ partial | Function and call patterns work. `$$$ARGS` inside a parameter list parses as variable-variable dereference; use a rule or literal parameter names |
| Python | `py` `pyi` | ✅ direct fixture | |
| Ruby | `rb` `gemspec` `rake` `ru` | ✅ matrix fixtures | Representative patterns; not every syntax construct |
| Rust | `rs` | ✅ direct fixture | |
| Scala | `sc` `sbt` `scala` | ✅ direct fixture | |
| Swift | `swift` | ✅ matrix fixture | Representative patterns; not every syntax construct |
| TypeScript/JavaScript | `ts` `tsx` `mts` `cts` `js` `jsx` `mjs` `cjs` | ✅ direct fixtures | |
| Zig | `zig` | ✅ direct fixture | |
| **Data/markup (structural, no functions):** CSS/SCSS, HTML, JSON/JSONC, SQL, TOML, YAML | `css` `scss` `htm` `html` `json` `jsonc` `sql` `toml` `yaml` `yml` | ✅ representative fixtures | CSS, SCSS, HTML, and SQL have direct pattern fixtures. HTML `<$TAG>` covers ordinary, script, style, and self-closing start tags while excluding tag-shaped raw text. The inventory matrix also exercises representative whole-source patterns for JSON, YAML, and TOML. Unknown YAML-rule node kinds fail at compile time |

The default build registers **45 extensions across 24 grammar families**.
`packages/octocode-tools-core/tests/tools/localGrammarMatrix.test.ts` exercises
the full extension inventory through public tools. Its 715 cases cover
representative syntax, views, and continuations; they do not prove every grammar
construct or minification transformation correct.

## Signature extraction / graph facts — `minify:"symbols"`, `localAnalyzeGraph`

`localGetFileContent minify:"symbols"` provides skeleton outlines. JS/TS uses the native `oxc` parser; other supported code languages use Tree-sitter body queries. Graph facts are syntax-derived and vary by language; signature capability does not establish complete declaration or call extraction.

Cross-file graph linking covers JavaScript/TypeScript ESM and binding-safe CommonJS, Rust modules, bounded Python absolute and relative imports, and quoted relative C/C++ includes. Explicit relative `package.json` imports become bounded metadata leaves. CommonJS links require an unshadowed literal `require`, `module.require`, or `createRequire(import.meta.url)` binding; dynamic, shadowed, reassigned, and otherwise ambiguous loaders remain coverage diagnostics. Python wildcards, ambiguous package attributes, and ambiguous stub layouts remain diagnostics. C/C++ system and macro includes are not linked. Other languages report unsupported cross-file linking rather than producing heuristic edges.

Supported (35 extensions in the default build): every code language in the
preceding structural table. The registered body queries determine signature
support; use the capability APIs for builds with optional features turned off.

The native engine has no Tree-sitter grammar for Elixir (`ex`/`exs`),
HCL/Terraform (`tf`/`hcl`/`tfvars`), Protobuf (`proto`), Bash/Shell
(`sh`/`bash`/`zsh`), Less (`less`), OCaml (`ml`/`mli`), Julia (`jl`), R (`r`),
Erlang (`erl`/`hrl`), Vue, Svelte, Astro, or Dart. Structural queries report
these extensions as unsupported; text search remains available. External
language-server resolution is independent of native grammar availability.

## Minification — file reads and search fragments

Minification support is separate from parser and LSP support. The default
configuration contains **151 extension entries**, plus 15
filename overrides. Many entries use comment and whitespace processing without
a syntax parser. Scala `.scala`, `.sc`, and `.sbt` share one strategy.

| View | Processing | Research use |
|---|---|---|
| `none` | Skips minification; extraction, security redaction, and response formatting still apply | Source evidence, comments, type declarations, edits, and literal matches |
| `standard` | Uses language-dependent processing. JS/TS uses OXC compact code generation without optimization, mangling, or type-declaration removal; other strategies compact JSON, markup, CSS, Markdown, or comments and whitespace | Orientation; use `none` for exact text, comments, and formatting |
| `symbols` | Extracts an outline for 35 code extensions; Markdown has a heading fallback. Unsupported or unavailable outlines fall back to `standard` | Declaration locations and source-line anchors; follow with an exact read for bodies |

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

Native regression coverage exercises all 151 configured extensions in standard
and full minification, all 15 filename overrides, and embedded script views.
The public file-read matrix exercises 45 extensions × three modes × two
`fullContent` settings × two readers, executing character continuations and
reconstructing each transformed view. This is representative syntax coverage,
not exhaustive language conformance.

History has a separate contract: PR details accept `none` or `standard` for
body, comments, reviews, and patches. Diff compaction is language-independent
and preserves changed source lines. Issue, commit, and compare details return
exact selected content and do not accept a file-view `minify` option.

Native minification can return the original input when the result is not
smaller, processing fails, or input exceeds its 1 MiB guard. A selected mode
does not establish which transformations ran. See the
[tool reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) for extraction and pagination.

## LSP — `lspGetSemantics`

Built-in routing covers 45 file extensions, 26 language IDs, and 20 server
commands. LSP routing is separate from native grammar support: Shell, Less, and
Elixir have LSP routes without structural grammars. Scala and TOML have
structural grammars without built-in LSP routes.

| Tier | Languages | What happens |
|---|---|---|
| **Bundled** | TypeScript/JavaScript, Python, YAML, JSON, HTML, CSS/SCSS/Less | Runs from packaged dependencies when no override or executable is found |
| **Auto-download** | Rust (rust-analyzer), C/C++ (clangd) | Downloaded + checksum-verified on first use |
| **Detect-and-instruct** | Shell (bash-language-server), PHP (intelephense), Go (gopls), Java (jdtls), Swift (sourcekit-lsp), C# (csharp-ls) | Needs the server or language toolchain installed; the error message gives an install or configuration hint |
| **PATH or override** | Ruby, Kotlin, Lua, SQL, Zig, Elixir | Resolves a known command from `PATH`, ecosystem locations, or a language-specific `OCTOCODE_*_SERVER_PATH` override |
| **Custom configuration** | Scala, TOML, and other file types | Register the extension, command, arguments, and language ID in `.octocode/lsp-servers.json` |

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

Production acceptance tests live in tools-core:
`tests/tools/lsp/routeMatrix.test.ts` checks all 45 extension routes, uppercase
extensions, and types without built-in routes; `productionMatrix.test.ts`
checks all 13 operations, missing servers, startup failures, missing capabilities,
and complete pagination unions with mutation/restart in both output formats.
These tests validate contracts with deterministic providers. They do not establish
that every external language server is installed or implements every operation.

For `workspaceSymbol`, provide `uri` as a language anchor in mixed-language
workspaces. The request accepts `workspaceRoot` without `uri`, but server
selection can otherwise depend on the first file returned by anchor discovery.

## Verify the shipped build

Use the compiled capability APIs for exact extension lists, then exercise the
public CLI or MCP tool path. Do not infer native grammar support from an LSP
route.

```bash
node -e "const n=require('./packages/octocode-engine/index.js'); \
console.log(n.getSupportedStructuralExtensions().sort()); \
console.log(n.getSupportedSignatureExtensions().sort())"

node packages/octocode/out/octocode.js tools localSearch \
    --queries '{"operation":"structural","path":"/ABS/REPO","pattern":"$$$"}'

node packages/octocode/out/octocode.js tools lspGetSemantics \
    --queries '{"uri":"/ABS/REPO/src/file.ts","type":"documentSymbols"}'
```

Run the package tests with these commands:

```bash
yarn workspace @octocodeai/octocode-engine test:rust
yarn workspace @octocodeai/octocode-engine test
yarn workspace @octocodeai/octocode-tools-core test
```

## `localSearch` text operation (ripgrep-backed)

| Feature | Values |
|---|---|
| `regex` | `smart` (default) · `fixed` (literal) · `perl` (lookaround/backreferences) |
| `caseMode` | `smart` · `sensitive` · `insensitive` |
| `wholeWord`, `invertMatch` | boolean |
| `multiline` | `off` · `on` · `dotall` (`.` spans newlines) |
| `resultView` | Text: `paginated` · `discovery` · `detailed` · `content` · `files` · `filesWithout` · `countLines` · `countMatches` · `matchOnly`; structural: `content` · `files` · `countMatches` |
| `unique` | `off` · `list` · `count` (requires `resultView:"matchOnly"`) |
| `sort` / `reverse` | `relevance` · `matchCount` · `path` · `modified` · `accessed` · `created`, all reversible |
| `include` / `exclude` / `excludeDir` | glob arrays |
| `maxDepth`, `contextLines`, `matchWindow`, `matchPage`, `maxMatchesPerFile` | bounds/pagination |

Read the live `localSearch` schema before scripting queries. The schema
validates fields per operation and rejects fields from other operation variants.
