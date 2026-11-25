# Toolchain

Load when choosing lint, format, typecheck, test, build, hooks or CI. Versions as of 2026-11-25; run `check-versions.mjs` on the final set. Each lane has a template: `apply-template.mjs --list`.

## Lint and format (JavaScript and TypeScript)

| Lane | Template | Use when | Watch out |
|---|---|---|---|
| Framework default | none | The framework CLI already set up linting (Next.js, Angular, SvelteKit): keep it | Add only the missing scripts (`format:check`, `typecheck`) |
| ESLint 10 + typescript-eslint + Prettier 3 | `lint-eslint` | You need ESLint's plugin ecosystem (React hooks, a11y, framework plugins) | Flat config only (`eslint.config.*`; `.eslintrc` was removed in v10). typescript-eslint 8 declares `typescript <6.1`: with TypeScript 7 pin `typescript@~6.0` or alias `typescript` to `@typescript/typescript6` and add `@typescript/native` for `tsc` |
| Oxlint + Oxfmt | `lint-oxc` | Speed in large repos; type-aware rules through `oxlint-tsgolint`, which tracks TypeScript 7 | Oxfmt is 0.x: pin it exactly. Oxlint skips files its ignore rules (including a parent `.gitignore`) exclude |
| Biome 2 | `lint-biome` | One fast tool and one config for lint, format and import sorting | Pin exactly; fewer plugin rules than ESLint. Use `rules.preset`; `rules.recommended` is deprecated |
| Vite+ | via `vp create` | A Vite app that wants one CLI (`vp check` = format + lint + types, `vp test`, `vp build`) | Built-in commands cannot be redefined; add package scripts that call them so `verify-project.mjs` finds the gates |

## Typecheck

TypeScript 7 (native, about 10× faster) is `typescript@latest`; run `tsc --noEmit` as the `typecheck` script. It ships no compiler API yet: tools that import `typescript` (typescript-eslint) need the TypeScript 6 API alias above. The template tsconfig is strict, with `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `verbatimModuleSyntax` and `erasableSyntaxOnly`.

## Test

| Need | Tool | Use when |
|---|---|---|
| Unit and integration, any TypeScript project | Vitest 5 (`@vitest/coverage-v8`) | Default; Vite-native, Jest-compatible API |
| Component tests | Vitest + Testing Library, `happy-dom` or `jsdom`; Vitest browser mode for real browsers | UI logic |
| End to end | Playwright | User flows across pages, browsers |
| HTTP mocking | MSW | Network boundaries in unit and component tests |
| Bun-only projects | `bun test` | Runtime is Bun and you want no extra tool |
| Rust | `cargo nextest run`; `cargo test --doc` for libraries | nextest is faster and isolates tests; it skips doctests |
| Python | pytest | Default |
| Go | `go test -race ./...` | Default; table-driven tests |

Write the first test with the first feature; `verify-project.mjs` counts a missing test gate as a failure.

## Build and package

Apps use the framework build. Libraries: tsdown (Rolldown) for ESM + `.d.ts`; check the package with `publint` and `@arethetypeswrong/cli`. Monorepo tasks: Turborepo or Nx (or `vp run` with Vite+). Dead code and unused dependencies: knip.

## Rust

`rust-toolchain.toml` pins stable with rustfmt and clippy (template `rust`). Add lints to Cargo.toml (`[workspace.lints]` in a workspace):

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
```

CI: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` (licenses, advisories, bans; create `deny.toml` with `cargo deny init`). Optional: `cargo llvm-cov` for coverage, `cargo machete` for unused dependencies.

## Python

uv for Python versions, environments, locking and running (template `python`). Ruff for lint and format (replaces flake8, isort, black). Type checker: mypy (stable default) or pyright; ty is 0.0.x, use it only by choice. pytest for tests.

## Go

`gofmt`, `go vet`, golangci-lint v2 (`version: "2"` config, template `go`), `go test -race`. Install golangci-lint from its release binaries.

## Hooks and CI

Hooks are optional and fast only: lefthook (any language) or husky + lint-staged; Vite+ has `vp staged`. CI runs the same gates as `verify-project.mjs`; every template ships a GitHub Actions workflow with `permissions: contents: read` and cancel-in-progress concurrency.
