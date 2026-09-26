# Canonical sources — the official Rust doc canon

Load when a best-practice, idiom, or API claim needs an authoritative anchor, or when deciding *which* official document settles a question. Why: this skill's discipline is evidence over reputation — advice should trace to the official canon or to upstream source (`octocode-research`), never to memory or registry popularity. The whole canon is also available offline via `rustup doc`.

## The official checklists — reach here first
| When the open question is… | Authoritative source | URL |
|---|---|---|
| Public API shape, naming, `must_use`, conversions, trait design | **Rust API Guidelines** (the official public-API checklist) | https://rust-lang.github.io/api-guidelines/ |
| Idioms, ownership, error handling, first-principles language model | **The Book** (*The Rust Programming Language*) | https://doc.rust-lang.org/book/ |
| Exact language semantics beyond the Book — "is this defined behavior?" | **The Reference** | https://doc.rust-lang.org/reference/ |
| `unsafe`, aliasing, UB, sound abstractions over raw pointers | **The Rustonomicon** | https://doc.rust-lang.org/nomicon/ |
| Cargo profiles, features, workspaces, publishing | **The Cargo Book** | https://doc.rust-lang.org/cargo/ |
| A specific compiler lint / diagnostic / codegen knob | **The rustc Book** + lint index | https://doc.rust-lang.org/rustc/ |
| Migrating an edition (2015→2018→2021→2024), what a bump changes | **The Edition Guide** | https://doc.rust-lang.org/edition-guide/ |
| Toolchain, channels (stable/beta/nightly), targets, MSRV workflow | **The rustup Book** | https://rust-lang.github.io/rustup/ |
| Documenting a crate (`cargo doc`, doctests, intra-doc links) | **The rustdoc Book** | https://doc.rust-lang.org/rustdoc/ |
| Standard-library API surface and guarantees | **std docs** | https://doc.rust-lang.org/std/ |

## Domain guides — for an application-area axis
| Domain | Source | URL |
|---|---|---|
| Command-line apps | **Command Line Book** | https://rust-cli.github.io/book/ |
| Embedded / microcontrollers | **Embedded Book** · **Discovery** · **Embedonomicon** | https://docs.rust-embedded.org/book/ · https://docs.rust-embedded.org/discovery/ · https://docs.rust-embedded.org/embedonomicon/ |
| WebAssembly | **wasm-bindgen Guide** (the rustwasm org was archived in 2025; its book is frozen) | https://wasm-bindgen.github.io/wasm-bindgen/ |
| Learning by doing / onboarding | **Rust by Example** · **Rustlings** | https://doc.rust-lang.org/rust-by-example/ · https://github.com/rust-lang/rustlings |

## How to use this canon
- **Anchor, don't quote from memory.** When you assert an idiom or API rule, name the canon document that backs it (the API Guidelines and the Reference are the two that *settle* disputes). If the claim is about a specific crate's behavior rather than the language, that is `octocode-research` territory — source at a ref, not a doc.
- **Toolchain baseline** (from the official install/getting-started flow): install and manage via **rustup**; `cargo` is the single entry point — `cargo new` / `build` / `run` / `test` / `doc` / `publish`, add deps with `cargo add`. `Cargo.lock` pins exact versions; keep the toolchain current with `rustup update`. This is the ground every other axis assumes.
- **When the canon and this skill disagree, the canon wins** — update the skill (via `octocode-skills`) and log why.

Next: for the idiom/API-surface rules these documents formalize, load `references/idioms.md`; for the unsafe review the Nomicon backs, `references/safety-and-security.md`; for edition/MSRV wiring, `references/workspace-manifest.md`.
