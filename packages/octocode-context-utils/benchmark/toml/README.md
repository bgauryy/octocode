# TOML (.toml)

Source sample: `toml/rust-cargo.toml`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 3039 | - | - |
| content-view | 1881 | 38.1% | 0.041 ms |
| applyMinification | 1885 | 38% | 0.034 ms |
| sync minify | 1885 | 38% | 0.032 ms |
| async minify | 1885 | 38% | 0.034 ms |
| symbols | n/a | n/a | 0.002 ms |

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
