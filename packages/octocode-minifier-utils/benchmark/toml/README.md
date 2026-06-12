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
| content-view | 1881 | 38.1% | 0.286 ms | 8.5/10 |
| applyMinification | 1885 | 38% | 0.269 ms | 8.5/10 |
| sync minify | 1885 | 38% | 0.251 ms | 8.5/10 |
| async minify | 1885 | 38% | 0.247 ms | 8.5/10 |
| symbols | n/a | n/a | 0.003 ms | n/a |

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
| symbols | n/a | n/a | n/a | n/a | n/a |

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
No symbols returned for this sample.
```
