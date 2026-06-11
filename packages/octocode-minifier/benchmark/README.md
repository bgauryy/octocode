# Real-Code Minifier Benchmark

Generated from local corpus: `/tmp/octocode-minifier-real-corpus`

This directory records before/after excerpts and metrics for one real sample per
discovered extension. Full third-party source files are not vendored here; use
the generator to recreate reports from a local corpus.

## Summary

- Agent-context minifier rating: **8.45/10**
- Minify rating: **8.31/10**
- Symbols rating where supported: **9.2/10** (27/27 returned)
- Average cuts: content-view 23.4%, apply 31.3%, async 32.9%
- Rating buckets: good 13, excellent 17, strong 12, fair 3

## Competitor Baseline

This benchmark rates Octocode as an agent-context compressor. Production
compiler and bundler minifiers are the right baseline for deployable output:

| Competitor | Best At | Octocode Position |
| --- | --- | --- |
| [Terser](https://www.npmjs.com/package/terser) | Production JavaScript parsing, compression, mangling, and formatting. | Used for JS/CJS/MJS and stronger JS-family paths where safe. |
| [esbuild](https://www.npmjs.com/package/esbuild) | Very fast JS/TS/CSS bundling and minification. | Better for production builds; Octocode avoids adding it as a runtime dependency. |
| [SWC](https://www.npmjs.com/package/@swc/core) | Rust-backed JS/TS compilation transforms. | Better compiler-grade path; Octocode uses TypeScript transform plus guarded minification. |
| [Lightning CSS](https://www.npmjs.com/package/lightningcss) | Parser-grade CSS transforms and minification. | Better production CSS optimizer; Octocode uses CleanCSS async and lightweight sync cleanup. |
| [html-minifier-terser](https://www.npmjs.com/package/html-minifier-terser) | HTML minification with embedded asset options. | Used for async HTML; content-view still prioritizes readable agent context. |

## Real Minification Type Matrix

Measured async result types across the real corpus: conservative 26, terser 4, aggressive 12, json 2, markdown 1.

| Ext | Format | Configured strategy | Async type | Content-view cut | Apply cut | Async cut | Symbols cut | Source |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| `js` | JavaScript | `terser` | `terser` | 9.6% | 21.6% | 21.6% | 61.3% | `js/ReactHooks.js` |
| `cjs` | CommonJS | `terser` | `terser` | 4.8% | 49.7% | 49.7% | 97.8% | `cjs/apidom-babel.config.cjs` |
| `mjs` | ESM JavaScript | `terser` | `terser` | 1.5% | 31.2% | 31.2% | 78.7% | `mjs/llhttp-eslint.config.mjs` |
| `jsx` | JSX | `terser` | `terser` | 0% | 20.6% | 20.6% | 93.3% | `jsx/vite-app.jsx` |
| `ts` | TypeScript | `conservative` | `conservative` | 29.6% | 67.9% | 67.9% | 69.1% | `ts/core.ts` |
| `tsx` | TSX | `conservative` | `conservative` | 0.4% | 0.4% | 0.4% | 48.4% | `tsx/next-index.tsx` |
| `json` | JSON | `json` | `json` | 0% | 29% | 29% | n/a | `json/typescript-package.json` |
| `jsonc` | JSONC | `json` | `json` | 0% | 15.2% | 15.2% | n/a | `jsonc/grammy-deno.jsonc` |
| `css` | CSS | `aggressive` | `aggressive` | 0.4% | 15.3% | 17.9% | 67.9% | `css/bootstrap.css` |
| `scss` | SCSS | `aggressive` | `aggressive` | 10.9% | 23.3% | 89.3% | 76.1% | `scss/_buttons.scss` |
| `html` | HTML | `aggressive` | `aggressive` | 0.3% | 12.5% | 15% | 28.4% | `html/vite-index.html` |
| `vue` | Vue | `aggressive` | `aggressive` | 0.8% | 5.9% | 7.6% | 26.9% | `vue/vite-app.vue` |
| `svelte` | Svelte | `aggressive` | `aggressive` | 0% | 21% | 21% | 87.1% | `svelte/vite-app.svelte` |
| `py` | Python | `conservative` | `conservative` | 21.9% | 21.9% | 21.9% | 85.3% | `py/sessions.py` |
| `java` | Java | `conservative` | `conservative` | 64.8% | 64.8% | 64.8% | 87.3% | `java/AnnotationUtils.java` |
| `go` | Go | `conservative` | `conservative` | 34.1% | 34.1% | 34.1% | 32.9% | `go/print.go` |
| `rs` | Rust | `conservative` | `conservative` | 62.2% | 62.2% | 62.2% | 92.5% | `rs/option.rs` |
| `c` | C | `conservative` | `conservative` | 3.8% | 3.8% | 3.8% | 88.8% | `c/git-add.c` |
| `cpp` | C++ | `conservative` | `conservative` | 30.2% | 30.2% | 30.2% | 80.6% | `cpp/llvm-raw-ostream.cpp` |
| `h` | C Header | `conservative` | `conservative` | 39% | 39% | 39% | 62.6% | `h/git-compat-util.h` |
| `hpp` | C++ Header | `conservative` | `conservative` | 38.3% | 38.3% | 38.3% | 71.5% | `hpp/fmt-color.hpp` |
| `cs` | C# | `conservative` | `conservative` | 28.3% | 28.3% | 28.3% | 73.5% | `cs/dotnet-argument-exception.cs` |
| `php` | PHP | `conservative` | `conservative` | 41.4% | 41.4% | 41.4% | 87.1% | `php/Arr.php` |
| `rb` | Ruby | `conservative` | `conservative` | 64.2% | 64.2% | 64.2% | 81.5% | `rb/blank.rb` |
| `sh` | Shell | `conservative` | `conservative` | 0.4% | 0.4% | 0.4% | 97.8% | `sh/nvm.sh` |
| `sql` | SQL | `conservative` | `conservative` | 35.6% | 35.6% | 35.6% | 93.9% | `sql/postgres-select.sql` |
| `yml` | YAML | `conservative` | `conservative` | 6.2% | 6.2% | 6.2% | n/a | `yaml/typescript-ci.yml` |
| `toml` | TOML | `conservative` | `conservative` | 38.1% | 38.1% | 38.1% | n/a | `toml/rust-cargo.toml` |
| `lua` | Lua | `aggressive` | `aggressive` | 15.6% | 27.7% | 27.7% | n/a | `lua/plenary-path.lua` |
| `graphql` | GraphQL | `conservative` | `conservative` | 3.2% | 3.2% | 3.2% | n/a | `graphql/graphql-go-kitchen-sink.graphql` |
| `md` | Markdown | `markdown` | `markdown` | 0.5% | 0.5% | 0.5% | n/a | `md/rust-readme.md` |
| `rst` | reStructuredText | `conservative` | `conservative` | 1.8% | 1.8% | 1.8% | n/a | `rst/cpython-tutorial-index.rst` |
| `scala` | Scala | `conservative` | `conservative` | 80.7% | 80.7% | 80.7% | 94.1% | `scala/Option.scala` |
| `swift` | Swift | `conservative` | `conservative` | 65.5% | 65.5% | 65.5% | 81.3% | `swift/Optional.swift` |
| `kt` | Kotlin | `conservative` | `conservative` | 49.1% | 49.1% | 49.1% | 77.2% | `kt/Collections.kt` |
| `dart` | Dart | `conservative` | `conservative` | 85.5% | 85.5% | 85.5% | n/a | `dart/dart-string.dart` |
| `r` | R | `aggressive` | `aggressive` | 46.6% | 57.3% | 57.3% | n/a | `r/dplyr-mutate.R` |
| `proto` | Protocol Buffers | `conservative` | `conservative` | 69.1% | 69.1% | 69.1% | n/a | `proto/protobuf-descriptor.proto` |

| Ext | Format | Report | Input bytes | Content-view cut | Apply cut | Sync cut | Async cut | Symbols cut | Agent rating |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `c` | C | `c/README.md` | 18107 | 3.8% | 3.8% | 3.8% | 3.8% | 88.8% | 7.6/10 good |
| `cjs` | CommonJS | `cjs/README.md` | 3184 | 4.8% | 49.7% | 49.7% | 49.7% | 97.8% | 9.7/10 excellent |
| `clj` | Clojure | `clj/README.md` | 276207 | 0.6% | 17% | 17% | 17% | n/a | 7.8/10 good |
| `cpp` | C++ | `cpp/README.md` | 32621 | 30.2% | 30.2% | 30.2% | 30.2% | 80.6% | 9/10 excellent |
| `cs` | C# | `cs/README.md` | 5603 | 28.3% | 28.3% | 28.3% | 28.3% | 73.5% | 8.7/10 strong |
| `css` | CSS | `css/README.md` | 280311 | 0.4% | 15.3% | 15.3% | 17.9% | 67.9% | 8.5/10 strong |
| `dart` | Dart | `dart/README.md` | 37049 | 85.5% | 85.5% | 85.5% | 85.5% | n/a | 9.5/10 excellent |
| `erl` | Erlang | `erl/README.md` | 123312 | 5.8% | 23.6% | 23.6% | 23.6% | n/a | 7.8/10 good |
| `ex` | Elixir | `ex/README.md` | 154291 | 1.5% | 16.2% | 16.2% | 16.2% | n/a | 7.8/10 good |
| `go` | Go | `go/README.md` | 33315 | 34.1% | 34.1% | 34.1% | 34.1% | 32.9% | 7.8/10 good |
| `graphql` | GraphQL | `graphql/README.md` | 1300 | 3.2% | 3.2% | 3.2% | 3.2% | n/a | 6.3/10 fair |
| `h` | C Header | `h/README.md` | 33059 | 39% | 39% | 39% | 39% | 62.6% | 8.7/10 strong |
| `hpp` | C++ Header | `hpp/README.md` | 25322 | 38.3% | 38.3% | 38.3% | 38.3% | 71.5% | 8.7/10 strong |
| `hs` | Haskell | `hs/README.md` | 41400 | 12.3% | 12.3% | 12.3% | 12.3% | n/a | 7/10 good |
| `html` | HTML | `html/README.md` | 359 | 0.3% | 12.5% | 12.5% | 15% | 28.4% | 7.7/10 good |
| `ini` | INI | `ini/README.md` | 7518 | 23.6% | 23.6% | 23.6% | 23.6% | n/a | 7.8/10 good |
| `java` | Java | `java/README.md` | 63265 | 64.8% | 64.8% | 64.8% | 64.8% | 87.3% | 9.7/10 excellent |
| `js` | JavaScript | `js/README.md` | 6864 | 9.6% | 21.6% | 21.6% | 21.6% | 61.3% | 8.5/10 strong |
| `json` | JSON | `json/README.md` | 3468 | 0% | 29% | 29% | 29% | n/a | 9/10 excellent |
| `jsonc` | JSONC | `jsonc/README.md` | 1427 | 0% | 15.2% | 15.2% | 15.2% | n/a | 8.3/10 strong |
| `jsx` | JSX | `jsx/README.md` | 3646 | 0% | 20.6% | 20.6% | 20.6% | 93.3% | 8.9/10 strong |
| `kt` | Kotlin | `kt/README.md` | 20559 | 49.1% | 49.1% | 49.1% | 49.1% | 77.2% | 9/10 excellent |
| `lua` | Lua | `lua/README.md` | 23250 | 15.6% | 27.7% | 27.7% | 27.7% | n/a | 8.5/10 strong |
| `md` | Markdown | `md/README.md` | 3304 | 0.5% | 0.5% | 0.5% | 0.5% | n/a | 6.3/10 fair |
| `mjs` | ESM JavaScript | `mjs/README.md` | 1259 | 1.5% | 31.2% | 31.2% | 31.2% | 78.7% | 9/10 excellent |
| `php` | PHP | `php/README.md` | 35469 | 41.4% | 41.4% | 41.4% | 41.4% | 87.1% | 9.4/10 excellent |
| `pl` | Perl | `pl/README.md` | 4523 | 16.8% | 31.4% | 31.4% | 31.4% | n/a | 8.5/10 strong |
| `pm` | Perl Module | `pm/README.md` | 5491 | 8.9% | 20.1% | 20.1% | 20.1% | n/a | 7.8/10 good |
| `proto` | Protocol Buffers | `proto/README.md` | 60347 | 69.1% | 69.1% | 69.1% | 69.1% | n/a | 9.5/10 excellent |
| `py` | Python | `py/README.md` | 34072 | 21.9% | 21.9% | 21.9% | 21.9% | 85.3% | 8.6/10 strong |
| `r` | R | `r/README.md` | 15796 | 46.6% | 57.3% | 57.3% | 57.3% | n/a | 9/10 excellent |
| `rb` | Ruby | `rb/README.md` | 3507 | 64.2% | 64.2% | 64.2% | 64.2% | 81.5% | 9.7/10 excellent |
| `rs` | Rust | `rs/README.md` | 100057 | 62.2% | 62.2% | 62.2% | 62.2% | 92.5% | 9.7/10 excellent |
| `rst` | reStructuredText | `rst/README.md` | 2616 | 1.8% | 1.8% | 1.8% | 1.8% | n/a | 6.3/10 fair |
| `scala` | Scala | `scala/README.md` | 20107 | 80.7% | 80.7% | 80.7% | 80.7% | 94.1% | 9.7/10 excellent |
| `scss` | SCSS | `scss/README.md` | 7057 | 10.9% | 23.3% | 23.3% | 89.3% | 76.1% | 9.7/10 excellent |
| `sh` | Shell | `sh/README.md` | 156857 | 0.4% | 0.4% | 0.4% | 0.4% | 97.8% | 7.6/10 good |
| `sql` | SQL | `sql/README.md` | 8415 | 35.6% | 35.6% | 35.6% | 35.6% | 93.9% | 9/10 excellent |
| `svelte` | Svelte | `svelte/README.md` | 2665 | 0% | 21% | 21% | 21% | 87.1% | 8.9/10 strong |
| `swift` | Swift | `swift/README.md` | 33805 | 65.5% | 65.5% | 65.5% | 65.5% | 81.3% | 9.7/10 excellent |
| `toml` | TOML | `toml/README.md` | 3039 | 38.1% | 38.1% | 38.1% | 38.1% | n/a | 8.5/10 strong |
| `ts` | TypeScript | `ts/README.md` | 92419 | 29.6% | 67.9% | 67.9% | 67.9% | 69.1% | 9.7/10 excellent |
| `tsx` | TSX | `tsx/README.md` | 285 | 0.4% | 0.4% | 0.4% | 0.4% | 48.4% | 7.2/10 good |
| `vue` | Vue | `vue/README.md` | 119 | 0.8% | 5.9% | 5.9% | 7.6% | 26.9% | 7.2/10 good |
| `yml` | YAML | `yml/README.md` | 12508 | 6.2% | 6.2% | 6.2% | 6.2% | n/a | 7/10 good |

## Coverage

- Configured extensions: 138
- Real corpus extensions covered here: 45
- Configured extensions missing from this corpus: 93

See `missing-real-samples.md` for formats that are
supported by the package but not present in this local corpus.

## Weakest Measured Formats

- `.graphql` GraphQL: 6.3/10 fair; content-view cut 3.2%, async cut 3.2%, symbols not returned.
- `.md` Markdown: 6.3/10 fair; content-view cut 0.5%, async cut 0.5%, symbols not returned.
- `.rst` reStructuredText: 6.3/10 fair; content-view cut 1.8%, async cut 1.8%, symbols not returned.
- `.hs` Haskell: 7/10 good; content-view cut 12.3%, async cut 12.3%, symbols not returned.
- `.yml` YAML: 7/10 good; content-view cut 6.2%, async cut 6.2%, symbols not returned.
- `.tsx` TSX: 7.2/10 good; content-view cut 0.4%, async cut 0.4%, symbols returned.
- `.vue` Vue: 7.2/10 good; content-view cut 0.8%, async cut 7.6%, symbols returned.
- `.c` C: 7.6/10 good; content-view cut 3.8%, async cut 3.8%, symbols returned.
- `.sh` Shell: 7.6/10 good; content-view cut 0.4%, async cut 0.4%, symbols returned.
- `.html` HTML: 7.7/10 good; content-view cut 0.3%, async cut 15%, symbols returned.

## Regenerate

```bash
yarn build
node benchmark/generate-real-code-report.mjs /path/to/real/corpus
```
