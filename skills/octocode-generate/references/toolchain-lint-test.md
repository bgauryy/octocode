Load when choosing a JavaScript or TypeScript lint and format lane, the `typecheck` script, test tools for any ecosystem, or build, packaging and unused-code tools. [Toolchain](toolchain.md) holds the gates per ecosystem, the templates and the version check.

# Toolchain: lint, typecheck, test and build

## Lint and format (JavaScript and TypeScript)

| Lane | Template | Use when | Watch |
|---|---|---|---|
| Framework default | none | The scaffolder set it up: Next.js (ESLint or Biome), Vite React (Oxlint), NestJS (Oxlint + Prettier), Expo (`expo lint`), SvelteKit (`--add eslint prettier`) | Keep its versions and add only the missing pieces: `format-prettier` for `format:check`, a `typecheck` script. Upgrading its linter can break its config's plugins |
| ESLint 10 + typescript-eslint 8 + Prettier 3 | `lint-eslint` | You need ESLint plugins (React hooks, a11y, framework rules) | Flat config only. typescript-eslint declares `typescript <6.1`: pin TypeScript to `check-versions.mjs`'s `fixPeer` (the latest 6.x) |
| Oxlint + Oxfmt | `lint-oxc` | Speed in large repositories; type-aware rules through `oxlint-tsgolint`, which tracks TypeScript 7 | Oxfmt is 0.x: pin it exactly. Oxlint skips files its ignore rules exclude, including a parent `.gitignore` |
| Biome 2 | `lint-biome` | One tool and one config for lint, format and import sorting | Pin exactly; fewer plugin rules than ESLint. Use `rules.preset`; `rules.recommended` is deprecated |
| Vite+ | via `vp create` | A Vite app that wants one CLI (`vp check` = format + lint + types) | Add the gate scripts the [Vite+ row](stacks-web.md#spa-and-content-sites) names |

## TypeScript

`typescript@latest` is TypeScript 7 (native). The `typecheck` script is `tsc -b --noEmit`: it also checks solution-style configs (`"files": []` plus `references`, as Vite generates), where `tsc --noEmit` checks nothing and passes. The template tsconfig is strict with `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `verbatimModuleSyntax`, `erasableSyntaxOnly` and `rewriteRelativeImportExtensions`, so `node src/index.ts` runs directly and `.ts` imports compile to `.js`. Tools that import the compiler API (typescript-eslint) still need TypeScript 6; its tsgo backend for 7.1 is experimental.

## Test

| Need | Tool |
|---|---|
| Unit and integration (TypeScript) | Vitest 5 with `@vitest/coverage-v8`. In a Vite app put `test` options in `vite.config.*`: a `vitest.config.*` replaces it, dropping the app's plugins and aliases |
| Components | Vitest + Testing Library with `happy-dom` or `jsdom`; Vitest browser mode for real browsers |
| End to end | Playwright |
| HTTP mocking | MSW |
| React Native | jest-expo (Expo) or the template's Jest |
| Other ecosystems | `cargo nextest` (+ `cargo test --doc` for libraries), pytest, `go test -race`, `flutter test`, xUnit, JUnit through Gradle, Swift Testing, Minitest or RSpec, Pest, ExUnit |

Keep tests minimal and meaningful: one test per core feature through its public seam (a route handler, a hook, a service function), plus one smoke test that the main flow renders or responds. Integration tests follow the [integrations](integrations.md) test rule. Add end-to-end tests only for a critical path (sign-in, checkout) or when the user asks. A missing test gate fails verification, so the first test arrives with the first feature.

## Build, package and unused code

Apps use the framework build. TypeScript libraries: tsdown (ESM + `.d.ts`), checked with `publint` and `@arethetypeswrong/cli`. Monorepo tasks: Turborepo or Nx. Unused files and dependencies: knip (TypeScript), `cargo machete` (Rust).
