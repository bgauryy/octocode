# TOML (.toml)

Source sample: `toml/rust-cargo.toml`

Strategy: `conservative`

Agent rating: **8.5/10 (strong)**

Agent understanding from minified output: **9.6/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 3039 | - | - | - |
| content-view | 1881 | 38.1% | 0.238 ms | 8.5/10 |
| applyMinification | 1885 | 38% | 0.235 ms | 8.5/10 |
| sync minify | 1885 | 38% | 0.256 ms | 8.5/10 |
| async minify | 1885 | 38% | 0.275 ms | 8.5/10 |
| symbols | 3383 | -11.3% | 0.16 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 9/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 3039 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 1881 | 38.1% | 9.6/10 excellent | 10/10 | 10/10 |
| minify | 1885 | 38% | 9.6/10 excellent | 10/10 | 10/10 |
| symbols | 3383 | -11.3% | 6.3/10 fair | 3.3/10 | 10/10 |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```toml
[workspace]
resolver = "2"
members = [
# tidy-alphabetical-start
  "compiler/rustc",
  "src/build_helper",
  "src/rustc-std-workspace/rustc-std-workspace-alloc",
  "src/rustc-std-workspace/rustc-std-workspace-core",
  "src/rustc-std-workspace/rustc-std-workspace-std",
  "src/rustdoc-json-types",
  "src/tools/build-manifest",
  "src/tools/bump-stage0",
  "src/tools/cargotest",
  "src/tools/clippy",
  "src/tools/clippy/clippy_dev",
  "src/tools/collect-license-metadata",
  "src/tools/compiletest",
  "src/tools/coverage-dump",
  "src/tools/features-status-dump",
  "src/tools/generate-copyright",
  "src/tools/generate-windows-sys",
  "src/tools/html-checker",
  "src/tools/jsondocck",
  "src/tools/jsondoclint",
  "src/tools/linkchecker",
  "src/tools/lint-docs",
  "src/tools/lld-wrapper",
  "src/tools/llvm-bitcode-linker",
  "src/tools/miri",
  "src/tools/miri/cargo-miri",
  "src/tools/miropt-test-tools",
  "src/tools/opt-dist",
  "src/tools/remote-test-client",
  "src/tools/remote-test-server",
  "src/tools/replace-version-placeholder",
  "src/tools/run-make-support",
  "src/tools/rust-installer",
  "src/tools/rustdoc",
  "src/tools/rustdoc-gui-test",
  "src/tools/rustdoc-themes",
  "src/tools/rustfmt",
  "sr

... [truncated 1239 chars] ...

Bigint libraries are slow without optimization, speed up testing
[profile.dev.package.test-float-parse]
opt-level = 3

# Speed up the binary as much as possible
[profile.release.package.test-float-parse]
opt-level = 3
codegen-units = 1
# FIXME: LTO cannot be enabled for binaries in a workspace
# <https://github.com/rust-lang/cargo/issues/9330>
# lto = true

# If you want to use a crate with local modifications, you can set a path or git dependency here.
# For git dependencies, also add your source to ALLOWED_SOURCES in src/tools/tidy/src/extdeps.rs.
#[patch.crates-io]


```

## Content-View Excerpt

```toml
[workspace]
resolver = "2"
members = [

  "compiler/rustc",
  "src/build_helper",
  "src/rustc-std-workspace/rustc-std-workspace-alloc",
  "src/rustc-std-workspace/rustc-std-workspace-core",
  "src/rustc-std-workspace/rustc-std-workspace-std",
  "src/rustdoc-json-types",
  "src/tools/build-manifest",
  "src/tools/bump-stage0",
  "src/tools/cargotest",
  "src/tools/clippy",
  "src/tools/clippy/clippy_dev",
  "src/tools/collect-license-metadata",
  "src/tools/compiletest",
  "src/tools/coverage-dump",
  "src/tools/features-status-dump",
  "src/tools/generate-copyright",
  "src/tools/generate-windows-sys",
  "src/tools/html-checker",
  "src/tools/jsondocck",
  "src/tools/jsondoclint",
  "src/tools/linkchecker",
  "src/tools/lint-docs",
  "src/tools/lld-wrapper",
  "src/tools/llvm-bitcode-linker",
  "src/tools/miri",
  "src/tools/miri/cargo-miri",
  "src/tools/miropt-test-tools",
  "src/tools/opt-dist",
  "src/tools/remote-test-client",
  "src/tools/remote-test-server",
  "src/tools/replace-version-placeholder",
  "src/tools/run-make-support",
  "src/tools/rust-installer",
  "src/tools/rustdoc",
  "src/tools/rustdoc-gui-test",
  "src/tools/rustdoc-themes",
  "src/tools/rustfmt",
  "src/tools/test-float-parse"

... [truncated 81 chars] ...

tor",
  "src/tools/unstable-book-gen",
  "src/tools/wasm-component-ld",
  "src/tools/x",

]

exclude = [
  "build",
  "compiler/rustc_codegen_cranelift",
  "compiler/rustc_codegen_gcc",
  "src/bootstrap",
  "tests/rustdoc-gui",

  "obj",
]

[profile.release.package.rustc_thread_pool]

overflow-checks = false

[profile.release.package.lld-wrapper]
debug = 0
strip = true
[profile.release.package.wasm-component-ld-wrapper]
debug = 0
strip = true

[profile.dev.package.test-float-parse]
opt-level = 3

[profile.release.package.test-float-parse]
opt-level = 3
codegen-units = 1
```

## Apply Minification Excerpt

```toml
[workspace]
resolver = "2"
members = [

  "compiler/rustc",
  "src/build_helper",
  "src/rustc-std-workspace/rustc-std-workspace-alloc",
  "src/rustc-std-workspace/rustc-std-workspace-core",
  "src/rustc-std-workspace/rustc-std-workspace-std",
  "src/rustdoc-json-types",
  "src/tools/build-manifest",
  "src/tools/bump-stage0",
  "src/tools/cargotest",
  "src/tools/clippy",
  "src/tools/clippy/clippy_dev",
  "src/tools/collect-license-metadata",
  "src/tools/compiletest",
  "src/tools/coverage-dump",
  "src/tools/features-status-dump",
  "src/tools/generate-copyright",
  "src/tools/generate-windows-sys",
  "src/tools/html-checker",
  "src/tools/jsondocck",
  "src/tools/jsondoclint",
  "src/tools/linkchecker",
  "src/tools/lint-docs",
  "src/tools/lld-wrapper",
  "src/tools/llvm-bitcode-linker",
  "src/tools/miri",
  "src/tools/miri/cargo-miri",
  "src/tools/miropt-test-tools",
  "src/tools/opt-dist",
  "src/tools/remote-test-client",
  "src/tools/remote-test-server",
  "src/tools/replace-version-placeholder",
  "src/tools/run-make-support",
  "src/tools/rust-installer",
  "src/tools/rustdoc",
  "src/tools/rustdoc-gui-test",
  "src/tools/rustdoc-themes",
  "src/tools/rustfmt",
  "src/tools/test-float-parse"

... [truncated 85 chars] ...

,
  "src/tools/unstable-book-gen",
  "src/tools/wasm-component-ld",
  "src/tools/x",

]

exclude = [
  "build",
  "compiler/rustc_codegen_cranelift",
  "compiler/rustc_codegen_gcc",
  "src/bootstrap",
  "tests/rustdoc-gui",

  "obj",
]

[profile.release.package.rustc_thread_pool]


overflow-checks = false


[profile.release.package.lld-wrapper]
debug = 0
strip = true
[profile.release.package.wasm-component-ld-wrapper]
debug = 0
strip = true


[profile.dev.package.test-float-parse]
opt-level = 3


[profile.release.package.test-float-parse]
opt-level = 3
codegen-units = 1
```

## Sync Minify Excerpt

```toml
[workspace]
resolver = "2"
members = [

  "compiler/rustc",
  "src/build_helper",
  "src/rustc-std-workspace/rustc-std-workspace-alloc",
  "src/rustc-std-workspace/rustc-std-workspace-core",
  "src/rustc-std-workspace/rustc-std-workspace-std",
  "src/rustdoc-json-types",
  "src/tools/build-manifest",
  "src/tools/bump-stage0",
  "src/tools/cargotest",
  "src/tools/clippy",
  "src/tools/clippy/clippy_dev",
  "src/tools/collect-license-metadata",
  "src/tools/compiletest",
  "src/tools/coverage-dump",
  "src/tools/features-status-dump",
  "src/tools/generate-copyright",
  "src/tools/generate-windows-sys",
  "src/tools/html-checker",
  "src/tools/jsondocck",
  "src/tools/jsondoclint",
  "src/tools/linkchecker",
  "src/tools/lint-docs",
  "src/tools/lld-wrapper",
  "src/tools/llvm-bitcode-linker",
  "src/tools/miri",
  "src/tools/miri/cargo-miri",
  "src/tools/miropt-test-tools",
  "src/tools/opt-dist",
  "src/tools/remote-test-client",
  "src/tools/remote-test-server",
  "src/tools/replace-version-placeholder",
  "src/tools/run-make-support",
  "src/tools/rust-installer",
  "src/tools/rustdoc",
  "src/tools/rustdoc-gui-test",
  "src/tools/rustdoc-themes",
  "src/tools/rustfmt",
  "src/tools/test-float-parse"

... [truncated 85 chars] ...

,
  "src/tools/unstable-book-gen",
  "src/tools/wasm-component-ld",
  "src/tools/x",

]

exclude = [
  "build",
  "compiler/rustc_codegen_cranelift",
  "compiler/rustc_codegen_gcc",
  "src/bootstrap",
  "tests/rustdoc-gui",

  "obj",
]

[profile.release.package.rustc_thread_pool]


overflow-checks = false


[profile.release.package.lld-wrapper]
debug = 0
strip = true
[profile.release.package.wasm-component-ld-wrapper]
debug = 0
strip = true


[profile.dev.package.test-float-parse]
opt-level = 3


[profile.release.package.test-float-parse]
opt-level = 3
codegen-units = 1
```

## Async Minify Excerpt

```toml
[workspace]
resolver = "2"
members = [

  "compiler/rustc",
  "src/build_helper",
  "src/rustc-std-workspace/rustc-std-workspace-alloc",
  "src/rustc-std-workspace/rustc-std-workspace-core",
  "src/rustc-std-workspace/rustc-std-workspace-std",
  "src/rustdoc-json-types",
  "src/tools/build-manifest",
  "src/tools/bump-stage0",
  "src/tools/cargotest",
  "src/tools/clippy",
  "src/tools/clippy/clippy_dev",
  "src/tools/collect-license-metadata",
  "src/tools/compiletest",
  "src/tools/coverage-dump",
  "src/tools/features-status-dump",
  "src/tools/generate-copyright",
  "src/tools/generate-windows-sys",
  "src/tools/html-checker",
  "src/tools/jsondocck",
  "src/tools/jsondoclint",
  "src/tools/linkchecker",
  "src/tools/lint-docs",
  "src/tools/lld-wrapper",
  "src/tools/llvm-bitcode-linker",
  "src/tools/miri",
  "src/tools/miri/cargo-miri",
  "src/tools/miropt-test-tools",
  "src/tools/opt-dist",
  "src/tools/remote-test-client",
  "src/tools/remote-test-server",
  "src/tools/replace-version-placeholder",
  "src/tools/run-make-support",
  "src/tools/rust-installer",
  "src/tools/rustdoc",
  "src/tools/rustdoc-gui-test",
  "src/tools/rustdoc-themes",
  "src/tools/rustfmt",
  "src/tools/test-float-parse"

... [truncated 85 chars] ...

,
  "src/tools/unstable-book-gen",
  "src/tools/wasm-component-ld",
  "src/tools/x",

]

exclude = [
  "build",
  "compiler/rustc_codegen_cranelift",
  "compiler/rustc_codegen_gcc",
  "src/bootstrap",
  "tests/rustdoc-gui",

  "obj",
]

[profile.release.package.rustc_thread_pool]


overflow-checks = false


[profile.release.package.lld-wrapper]
debug = 0
strip = true
[profile.release.package.wasm-component-ld-wrapper]
debug = 0
strip = true


[profile.dev.package.test-float-parse]
opt-level = 3


[profile.release.package.test-float-parse]
opt-level = 3
codegen-units = 1
```

## Symbols

```txt
 1| [workspace]
 2| resolver = "2"
 3| members = [
 4| # tidy-alphabetical-start
 5|   "compiler/rustc",
 6|   "src/build_helper",
 7|   "src/rustc-std-workspace/rustc-std-workspace-alloc",
 8|   "src/rustc-std-workspace/rustc-std-workspace-core",
 9|   "src/rustc-std-workspace/rustc-std-workspace-std",
10|   "src/rustdoc-json-types",
11|   "src/tools/build-manifest",
12|   "src/tools/bump-stage0",
13|   "src/tools/cargotest",
14|   "src/tools/clippy",
15|   "src/tools/clippy/clippy_dev",
16|   "src/tools/collect-license-metadata",
17|   "src/tools/compiletest",
18|   "src/tools/coverage-dump",
19|   "src/tools/features-status-dump",
20|   "src/tools/generate-copyright",
21|   "src/tools/generate-windows-sys",
22|   "src/tools/html-checker",
23|   "src/tools/jsondocck",
24|   "src/tools/jsondoclint",
25|   "src/tools/linkchecker",
26|   "src/tools/lint-docs",
27|   "src/tools/lld-wrapper",
28|   "src/tools/llvm-bitcode-linker",
29|   "src/tools/miri",
30|   "src/tools/miri/cargo-miri",
31|   "src/tools/miropt-test-tools",
32|   "src/tools/opt-dist",
33|   "src/tools/remote-test-client",
34|   "src/tools/remote-test-server",
35|   "src/tools/replace-version-placeholder",
36|   "src/tools/run-make-support",
37|   "src/tools/rust-installer",
38|   "src/tools/rustdoc",
39|   "src/tools/rustdoc-gui-test",
40|   "src/tools/rustdoc-themes",
41|   "src/tools/rustfmt",
42|   "src/tools/test-float-parse",
43|   "src/tools/tidy",
44|   "src/tools/tier-check",
45|   "src/tools/unicode-table-generator",
46|   "src/tools/unstable-book-gen",
47|   "src/tools/wasm-component-ld",
48|   "src/tools/x",
49| # tidy-alphabetical-end
50| ]
52| exclude = [
53|   "build",
54|   "compiler/rustc_codegen_cranelift",
55|   "compiler/rustc_codegen_gcc",
56|   "src/boo

... [truncated 783 chars] ...

ry user needs to download (and 15MB on disk).
73| [profile.release.package.lld-wrapper]
74| debug = 0
75| strip = true
76| [profile.release.package.wasm-component-ld-wrapper]
77| debug = 0
78| strip = true
80| # Bigint libraries are slow without optimization, speed up testing
81| [profile.dev.package.test-float-parse]
82| opt-level = 3
84| # Speed up the binary as much as possible
85| [profile.release.package.test-float-parse]
86| opt-level = 3
87| codegen-units = 1
88| # FIXME: LTO cannot be enabled for binaries in a workspace
89| # <https://github.com/rust-lang/cargo/issues/9330>
90| # lto = true
92| # If you want to use a crate with local modifications, you can set a path or git dependency here.
93| # For git dependencies, also add your source to ALLOWED_SOURCES in src/tools/tidy/src/extdeps.rs.
94| #[patch.crates-io]
```
