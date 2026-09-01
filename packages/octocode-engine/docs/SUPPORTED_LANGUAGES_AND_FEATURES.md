# Supported languages & features

Reference, not a tutorial. Regenerate the extension lists from the engine itself if this ever looks stale:

```bash
node -e "const n=require('./packages/octocode-engine/index.js');
console.log('structural', n.getSupportedStructuralExtensions().sort());
console.log('signatures', n.getSupportedSignatureExtensions().sort());
console.log('jsts', n.getSupportedJsTsExtensions().sort());"
```

## Structural (AST) search — `localSearch operation:"structural"`

Tree-sitter-backed. Two query forms: `pattern` (code-shaped, `$X`/`$$$ARGS` metavars) and `rule` (YAML, `kind`/`has`/`inside`/`all`/`any`/`not`). **`rule: kind: <node_kind>` always works** — it doesn't go through pattern-fragment parsing at all. Reach for it whenever `pattern` misbehaves.

| Language | Extensions | `pattern` mode | Notes |
|---|---|---|---|
| Bash/Shell | `sh` `bash` `zsh` | ✅ full | `$NAME` at a bare-word position (function name) uses `_` internally, transparent to callers |
| C | `c` `h` | ✅ full | |
| C++ | `cc` `cpp` `cxx` `hh` `hpp` `hxx` | ⚠️ partial | `int $NAME(...) { ... }` parses as a variable declaration (C++11 list-init ambiguity), not a function — use `rule: kind: function_definition` |
| C# | `cs` | ✅ full | Every pattern is parsed inside a synthetic wrapper class for real member context (transparent) |
| Elixir | `ex` `exs` | ✅ full | |
| Erlang | `erl` `hrl` | ✅ full | |
| Go | `go` | ✅ full | |
| HCL/Terraform | `hcl` `tf` `tfvars` | ❌ pattern mode | `resource`/`variable` block patterns don't match, even fully literal — use `rule: kind: block` |
| Java | `java` | ✅ full | |
| Julia | `jl` | ✅ full | |
| Kotlin | `kt` `kts` | ✅ full | |
| Lua | `lua` | ✅ full | |
| OCaml | `ml` `mli` | ✅ full | |
| PHP | `php` | ⚠️ partial | Function/call patterns work (incl. bare-word function-name position); `$$$ARGS` **inside a parameter list** specifically doesn't (PHP parses repeated `$` as variable-variable dereference) — use `rule` or literal param names |
| Protocol Buffers | `proto` | ❌ pattern mode | `message $NAME { ... }` errors outright; `rpc` patterns don't match even literal — use `rule: kind: message` / `rule: kind: rpc` |
| Python | `py` `pyi` | ✅ full | |
| R | `r` | ✅ full | |
| Ruby | `rb` `gemspec` `rake` `ru` | ✅ full | |
| Rust | `rs` | ✅ full | |
| Scala | `sc` `sbt` `scala` | ✅ full | |
| Swift | `swift` | ✅ full | |
| TypeScript/JavaScript | `ts` `tsx` `mts` `cts` `js` `jsx` `mjs` `cjs` | ✅ full | |
| Zig | `zig` | ✅ full | |
| **Data/markup (structural, no functions):** CSS/SCSS/Less, HTML, JSON/JSONC, SQL, TOML, YAML | `css` `scss` `less` `htm` `html` `json` `jsonc` `sql` `toml` `yaml` `yml` | ✅ | Selector/key/tag/table shapes, not function bodies. Literal patterns occasionally fail to parse where a `rule` or a metavar-based pattern succeeds — try both |

## Signature extraction / graph facts — `minify:"symbols"`, `localAnalyzeGraph`

`localGetFileContent minify:"symbols"` (skeleton outlines) and `localAnalyzeGraph` share this coverage. JS/TS uses the native `oxc` parser; everything else uses the tree-sitter body-query grammar.

Supported (47 extensions): every language in the table above **except** the data/markup row (CSS/SCSS/Less/HTML/JSON/SQL/TOML/YAML have no function bodies to extract) — **and, as a known gap, Julia (`jl`) and OCaml (`mli`/`ml`)**, which support structural search but not signature skeletons or graph facts yet.

## LSP — `lspGetSemantics`

24 languages have a native server-resolution entry (`c cpp csharp css elixir go html java javascript javascriptreact json kotlin less lua php python ruby rust scss sql swift typescript typescriptreact yaml zig`). Resolution ladder, cheapest first:

| Tier | Languages | What happens |
|---|---|---|
| **Bundled (zero install)** | TypeScript/JavaScript, Python (pyright), Shell (bash-language-server), YAML, JSON, HTML, CSS, PHP (intelephense) | Runs out of the box, no external binary |
| **Auto-download** | Rust (rust-analyzer), C/C++ (clangd) | Downloaded + checksum-verified on first use |
| **Toolchain-coupled (detect-and-instruct)** | Go (gopls), Java (jdtls), Swift (sourcekit-lsp), C# (csharp-ls) | Needs the language's own toolchain installed; error message gives the exact install command |
| **Bring-your-own** | Kotlin, Ruby, Lua, SCSS/Less, SQL, Zig, Elixir, and anything else | Needs a server already on `PATH`, or set `OCTOCODE_<LANG>_SERVER_PATH` |

`documentSymbols`/`definition`/`references`/`hover`/etc. degrade gracefully to a typed `empty` payload (`unsupportedOperation`, `noLocations`, ...) when a server lacks a capability — never a crash.

## `localSearch` text operation (ripgrep-backed)

| Feature | Values |
|---|---|
| `regex` | `smart` (default) · `fixed` (literal) · `perl` (lookaround/backreferences) |
| `caseMode` | `smart` · `sensitive` · `insensitive` |
| `wholeWord`, `invertMatch` | boolean |
| `multiline` | `off` · `on` · `dotall` (`.` spans newlines) |
| `output` | `content` · `files` · `filesWithout` · `countLines` · `countMatches` · `matchOnly` |
| `unique` | `off` · `list` · `count` (requires `output:"matchOnly"`) |
| `sort` / `sortReverse` | `relevance` · `matchCount` · `path` · `modified` · `accessed` · `created`, all reversible |
| `include` / `exclude` / `excludeDir` | glob arrays |
| `maxDepth`, `contextLines`, `matchWindow`, `matchPage`, `maxMatchesPerFile` | bounds/pagination |

All of the above verified end-to-end against a live build; `sort`/`sortReverse` combinations for every mode (not just filesystem sorts) confirmed working.
