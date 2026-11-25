# Toolchain

Load when choosing lint, format, typecheck, test, build, hooks or CI. Versions as of 2026-11-25: run `check-versions.mjs` on the final set (`--help` lists the 11 registries and their range syntax). `apply-template.mjs --list` shows each template; `ci-js` writes one CI step per gate script the project has, for any JavaScript framework. Typical sets: a framework app `base ci-js dependabot`, plus `format-prettier` when the scaffold set up no formatter; a Node.js TypeScript service or library `base node-ts ci-js <lint-lane> dependabot`; another stack `base <stack> dependabot`.

## Gates per ecosystem

`verify-project.mjs` derives these gates; CI runs the same commands. A gate the project lacks is `missing` with a `hint`: add it rather than skip it.

| Stack (marker) | format | lint | typecheck | test | build | Template |
|---|---|---|---|---|---|---|
| JavaScript (`package.json`) | `format:check` script | `lint` script | `typecheck` script | `test` script | `build` script | `ci-js`; plain Node.js TypeScript adds `node-ts`; a lint lane when the scaffold has none |
| Deno (`deno.json`) | `deno fmt --check` | `deno lint` | `deno check` | `deno test -A` | `deno task build` | `deno` |
| Rust (`Cargo.toml`) | `cargo fmt --check` | `cargo clippy -- -D warnings` | compile | `cargo nextest run` | `cargo build` | `rust` |
| Python (`pyproject.toml`) | `ruff format --check` | `ruff check` | mypy, pyright or ty | pytest | `uv build` | `python` |
| Go (`go.mod`) | `gofmt -l` | golangci-lint, else `go vet` | compile | `go test -race` | `go build` | `go` |
| Dart, Flutter (`pubspec.yaml`) | `dart format --set-exit-if-changed` | `analyze --fatal-infos` (also types) | | `flutter test` | `flutter build <platform>` | `flutter` |
| .NET (`.slnx`, `.csproj`) | `dotnet format --verify-no-changes` (also analyzers) | | compile | `dotnet test` (needs a test project) | `dotnet build -warnaserror` | `dotnet` |
| JVM (`build.gradle.kts`, `pom.xml`) | Spotless or ktlint | Android lint, detekt or Checkstyle | compile | `test` | `assemble` / `package` | `gradle` |
| Swift (`Package.swift`) | `swift format lint --strict` (also lint); SwiftLint when configured | | compile | `swift test` | `swift build` | `swift` |
| Ruby (`Gemfile`) | RuboCop or Standard (also lint) | | Sorbet or Steep | `bin/rails test` or RSpec | | Rails generates CI |
| PHP (`composer.json`) | Pint or PHP-CS-Fixer | PHPStan/Larastan or Psalm (also types) | | `php artisan test`, Pest or PHPUnit | | |
| Elixir (`mix.exs`) | `mix format --check-formatted` | `mix credo` | Dialyzer | `mix test` | `mix compile --warnings-as-errors` | |

An Xcode project, a monorepo task or any other custom command: `verify-project.mjs <dir> --gate <gate>=<command>`.

## Lint and format (JavaScript and TypeScript)

| Lane | Template | Use when | Watch |
|---|---|---|---|
| Framework default | none | The scaffolder set it up: Next.js (ESLint or Biome), Vite React (Oxlint), NestJS (Oxlint + Prettier), Expo (`expo lint`), SvelteKit (`--add eslint prettier`) | Keep its versions and add only the missing pieces: `format-prettier` for `format:check`, a `typecheck` script. Upgrading its linter can break its config's plugins |
| ESLint 10 + typescript-eslint 8 + Prettier 3 | `lint-eslint` | You need ESLint plugins (React hooks, a11y, framework rules) | Flat config only. typescript-eslint declares `typescript <6.1`: pin TypeScript to `check-versions.mjs`'s `fixPeer` (the latest 6.x) |
| Oxlint + Oxfmt | `lint-oxc` | Speed in large repositories; type-aware rules through `oxlint-tsgolint`, which tracks TypeScript 7 | Oxfmt is 0.x: pin it exactly. Oxlint skips files its ignore rules exclude, including a parent `.gitignore` |
| Biome 2 | `lint-biome` | One tool and one config for lint, format and import sorting | Pin exactly; fewer plugin rules than ESLint. Use `rules.preset`; `rules.recommended` is deprecated |
| Vite+ | via `vp create` | A Vite app that wants one CLI (`vp check` = format + lint + types) | Add package scripts that call `vp` so the gates are found |

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

Keep tests minimal and meaningful: one test per core feature through its public seam (a route handler, a hook, a service function), plus one smoke test that the main flow renders or responds. Test integrations against fakes or local emulators ([integrations](integrations.md)). Add end-to-end tests only for a critical path (sign-in, checkout) or when the user asks. A missing test gate fails verification, so the first test arrives with the first feature.

## Build, package and unused code

Apps use the framework build. TypeScript libraries: tsdown (ESM + `.d.ts`), checked with `publint` and `@arethetypeswrong/cli`. Monorepo tasks: Turborepo or Nx. Unused files and dependencies: knip (TypeScript), `cargo machete` (Rust).

## Rust, Python and Go

- Rust: `rust-toolchain.toml` pins stable with rustfmt and clippy. Add to Cargo.toml (`[workspace.lints]` in a workspace) `[lints.rust] unsafe_code = "forbid"` and `[lints.clippy] all = { level = "warn", priority = -1 }`. CI also runs `cargo deny check` (create `deny.toml` with `cargo deny init`).
- Python: uv manages Python, environments, locking and running. Ruff lints and formats. mypy is the stable type checker default; pyright works too; ty is 0.0.x, use it only by choice.
- Go: golangci-lint v2 (`version: "2"` config) from its release binaries, not `go install`.

## CI, dependencies and hooks

- Every template's workflow has `permissions: contents: read`, cancel-in-progress concurrency, and actions pinned to a commit SHA with the release in a comment, as GitHub's hardening guide recommends. Re-pin workflows a scaffolder generated ([research](research.md#recipe), step 7).
- The `dependabot` template writes `.github/dependabot.yml` with weekly updates for each detected stack and for GitHub Actions. Apply it after the manifests exist.
- Commit exactly one lockfile; CI installs from it (`npm ci`, `pnpm install --frozen-lockfile`, `uv sync --locked`, `cargo --locked`).
- Hooks are optional and must stay fast: lefthook (any language) or husky + lint-staged.
