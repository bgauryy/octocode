# Adding or splitting a crate

Large crates are split because one crate's frontend runs on a single thread. Extracting the lower layers (93k of 458k lines) into `runtime-base` cut a lower-layer unit-test edit from 218 to 34 CPU s; while tools and dispatch shared one crate, a one-line tool edit recompiled 8 units for 259–326 CPU s. New code goes in the crate that owns its seam or in a new leaf, never into a crate on the critical path for convenience.

## Split

- Cut along layers a guard already enforces: confirm the module graph is acyclic, then extract bottom-up. Prefer this to moving unit tests into a test crate, which keeps the library one serial unit and forces items `pub`.
- Write the move as a replayable script, not hand edits: a path map; rewrites of `use` trees, `super::` chains that cross the new root, and `#[path]` mounts; generated manifests; a compiler-guided loop that widens only what E0603, E0616, E0624 and E0451 name, logging each. A second run changes nothing, and a missing anchor exits non-zero naming the file. One split replayed over 1,073 files in 1 min 46 s, so it lands on the current tree.
- A re-export or root `use` that keeps old paths compiling is a bridge: rewrite its users, then delete it (one extraction left 2,112 bridged references). `unreachable_pub` then flags what stayed too wide.
- Generated types a lower crate names go in a leaf (`contract-base`, re-exported by `contract-types`): the lower crate then started at 0.4 s instead of 17.2 s.

## New crate, in one change

1. Manifest from a sibling (`crates/tool-ast/Cargo.toml`): workspace inheritance, `auto* = false`, `doctest = false`, a `test-support` feature if other crates' tests need its fixtures, `[lints] workspace = true`. Copy the siblings' cast and print lints into `src/lib.rs`.
2. Root `Cargo.toml`: `members`; `[profile.release.package.<crate>] codegen-units = 16` unless it holds hot algorithms; `[profile.dev.package.<crate>] debug = false` when it is large.
3. `CRATE_EDGES` in `crates/runtime/tests/architecture_guards.rs` and the crate list in `ARCHITECTURE.md`.
4. The test selection in `scripts/rust-tests.cjs`, the `dev-test` alias in `.cargo/config.toml` and `bacon.toml` (`tests/build-publication.test.cjs` keeps them equal); its `test-support` feature in `dev-unify`.
5. `clippy-baseline.json`: move the findings with `node skills-dev/octocode-dev/scripts/rust-lint.mjs --rekey <old crate> <new crate>` (`--dry-run` first); it refuses when per-lint totals would change.

Then run `$DEV check` and `$DEV test:dev --crate <crate>`, and when build time motivated the split, compare `--timings` by CPU to confirm the critical path moved.
