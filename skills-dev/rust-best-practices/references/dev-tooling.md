# Dev tooling — the Rust toolbelt

Load when setting up a dev loop, CI, or a new contributor, or when asking "is there a tool for X?". Why: most Rust quality/speed wins are one `cargo install` away; reinventing them in scripts is waste. Install prebuilt binaries fast with `cargo binstall <tool>`; verify a tool is maintained before adopting it.

## Toolchain pin (commit it)
```toml
# rust-toolchain.toml
[toolchain]
channel = "1.90"        # exact stable CI builds with; MSRV floor lives in rust-version
components = ["rustfmt", "clippy", "rust-analyzer", "llvm-tools"]
```
Built-ins to know: `cargo add/remove/info`, `cargo tree -d -i <crate>`, `cargo fix --edition`, `cargo doc --open`, `cargo build --timings`, `cargo metadata` (for scripts).

## The belt
| Job | Tool | Why |
|---|---|---|
| IDE | `rust-analyzer` | set `check.command = "clippy"` so the editor shows lints |
| Watch loop | `bacon` | background check/clippy/test; `cargo-watch` is in maintenance mode |
| Format | `rustfmt` (+ `rustfmt.toml`), `taplo fmt` for TOML, `cargo sort` for deps | zero-debate diffs |
| Lint | `clippy` with `[workspace.lints]`; `typos` for spelling | first review pass |
| Tests | `cargo nextest run` | per-test processes, parallel, retries, JUnit; run doctests via `cargo test --doc` |
| Snapshots | `insta` + `cargo insta review` | parser/CLI/serializer output |
| Property / fuzz | `proptest` · `cargo fuzz` (libFuzzer) | find the inputs you didn't think of |
| Coverage | `cargo llvm-cov nextest` | source-based, accurate, lcov/html |
| Test quality | `cargo mutants` | proves tests actually assert |
| UB / concurrency | `cargo +nightly miri test` · `loom` · `kani` | UB, interleavings, bounded proofs |
| Inspect codegen | `cargo expand` · `cargo asm` (cargo-show-asm) · `cargo llvm-lines` | macro output, inlining, generic bloat |
| Profile CPU / size | `samply` · `cargo flamegraph` · `perf` · `cargo bloat --release --crates` | build with a `profiling` profile; what fills the binary |
| Profile memory | `dhat-rs` · `heaptrack` | allocation counts/peaks — see `references/memory.md` |
| Bench / async | `criterion` / `divan` · `hyperfine` (CLI) · `tokio-console` | `--release` numbers; stuck tasks, busy polls |
| Unused deps / disk | `cargo machete` · `cargo shear` · `cargo udeps` (nightly) · `cargo sweep` | trim graph and stale `target/` |
| Features | `cargo hack --each-feature` / `--feature-powerset` | every combo compiles |
| API / MSRV | `cargo semver-checks` · `cargo msrv verify` | no accidental breaking release |
| Supply chain | `cargo deny` · `cargo audit` · `cargo vet` · `cargo auditable` | see `references/safety-and-security.md` |
| Release | `release-plz` / `cargo-release` · `cargo-dist` | version bump, changelog, binaries |

## Aliases = npm scripts
```toml
# .cargo/config.toml
[alias]
lint = "clippy --workspace --all-targets --all-features -- -D warnings"
t    = "nextest run --workspace"
xtask = "run -p xtask --"
```
Anything beyond one command goes in an `xtask` crate, not bash — see `references/workspace-crates.md`.

## CI order (fail fast, cheap first)
`fmt --check` → `clippy -D warnings` → `nextest` + `test --doc` → `deny check` → `hack --each-feature check` → `semver-checks` (libs) → MSRV build → Miri/fuzz on unsafe/parsers. Use `--locked` everywhere; cache with `Swatinem/rust-cache` (+ `sccache` per `references/build-profiles.md`).

Next: for memory profiling practice, load `references/memory.md`; for the security gate, `references/safety-and-security.md`.
