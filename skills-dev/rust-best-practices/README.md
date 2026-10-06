# Rust Best Practices

Write, structure, and harden Rust that a seasoned maintainer would approve. The skill inspects the real code and manifests before it advises, and it checks crate claims against upstream source, not registry popularity.

- **Structure:** `SKILL.md` holds the rules and one skill map (trigger → reference); `references/` holds 12 pages, one per decision area. No scripts.
- **Maintainer check:** run the `octocode-skills` review against this folder before shipping.
- **Repository practice:** covers generated contracts, runtime ownership, cancellation, lossless pagination, build features, and safe addon staging. Source anchors are in `references/sources-and-crates.md`.
- **Open choices:** compare an existing approach with one alternative and test the deciding unknown; route broader scope exploration to `octocode-brainstorming`.
