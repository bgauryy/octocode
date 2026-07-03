# Repo Benchmark Recipe

End-to-end benchmark for Octocode's text, AST, and LSP layers across five popular repos covering JavaScript/TypeScript, Rust, Java, C++, and Next.js.

## Setup

```bash
# 1. Clone all repos (once)
node benchmark/repo/clone.mjs

# 2. Run all tool probes
node benchmark/repo/run.mjs
```

Or use the package scripts:

```bash
yarn repo:clone
yarn repo:bench
```

## Repos

| Key | Repo | Tag | Language |
|-----|------|-----|----------|
| `zustand` | pmndrs/zustand | v5.0.5 | TypeScript (state management) |
| `tokio` | tokio-rs/tokio | tokio-1.45.0 | Rust |
| `spring-boot` | spring-projects/spring-boot | v3.5.3 | Java |
| `chromium` | chromium/src `base/` (sparse) | HEAD | C++ |
| `nextjs` | vercel/next.js | v15.3.3 | JavaScript/TypeScript |

Chromium is a sparse shallow clone of `base/` only (~250 MB vs 35 GB full tree).

## Reproducibility

`pins.json` records the exact SHA cloned for each repo. Run `clone.mjs` again with `--force` to reclone at a new HEAD, then commit the updated `pins.json`.

## Results

Results land in `results/repo/<name>/results.md` after `run.mjs` completes.

## Tool Layers Exercised

| Layer | Tool | What it proves |
|-------|------|----------------|
| text | `localSearchCode` (ripgrep) | Engine finds literals/regex in the repo |
| ast | `localSearchCode mode:"structural"` | Tree-sitter parses the language correctly |
| symbols | `lspGetSemantics type:"documentSymbols"` | OXC (JS/TS) or LSP outlines a key file |

LSP semantic queries (definition, references) are not part of the automated run because they require installed language servers and a warm indexing phase — those are covered separately by `benchmark/lsp/check-lsp-live.mjs`.
