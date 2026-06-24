# Context — External Measurement Tools

Covers what the native toolset can't prove: **measurement** (metrics, type-coverage %), **graph rules** (cycles, layers, instability), framework-aware dead code, linting, type checking, and security scanning. Reach here when a claim needs a number or framework-complete graph. See [checklist-quality-signals.md](./checklist-quality-signals.md) for the measurement signal catalog.

**Ask the user before running.** Use `npx` (JS/TS) or `pip`/`pipx` (Python) — avoid global installs.

## When to reach for which

| Native gap | Tool |
|------------|------|
| Dependency cycles, layer rules, instability/SDP | `dep-cruiser` |
| Framework-aware dead exports/files/deps | Smart OQL `target:"research"` first, `knip` to confirm framework graph semantics |
| Type-safety % (typed vs `any`) | `type-coverage` |
| Cyclomatic complexity threshold | `eslint --rule '{"complexity":["error",10]}'` |
| Type errors | `tsc` (TS) / `mypy` / `pyright` (Py) |
| Lint / style | `eslint` (JS/TS) / `ruff` (Py) |
| CSS issues | `stylelint` |
| Python security sinks | `bandit` |

## eslint

```bash
npx eslint --fix <target>
npx eslint --format json <target>
npx eslint --rule '{"complexity": ["error", 10]}' <target>
```

## tsc

```bash
npx tsc --noEmit
npx tsc --noEmit --strict
npx tsc --noEmit -p tsconfig.json --pretty --noErrorTruncation
```

## stylelint — CSS/SCSS/Less

Use when the project has CSS files (native AST/LSP target code, not styles).

```bash
npx stylelint "**/*.css"
npx stylelint "**/*.scss"
npx stylelint --fix "**/*.css"
npx stylelint --formatter json "**/*.css"
```

## knip — Framework-aware dead code

100+ plugins (Next.js, Remix, Angular) detect framework-specific usage that Smart OQL's candidate graph and LSP `references` + `ast` import search may not see. Use Smart OQL first to get `symbol/kind/file/line/directRefs/externalRefs/retainedBy/verdict` rows, then use knip when deletion safety depends on framework entrypoints, workspace/package rules, or dependency manifests.

```bash
npx knip
npx knip --exports
npx knip --dependencies
npx knip --files
npx knip --workspace packages/my-pkg
npx knip --fix
npx knip --reporter json
```

## type-coverage — Type safety %

Project-wide typed-vs-any ratio. `ast` finds per-file `any`; this gives one number for the whole codebase.

```bash
npx type-coverage
npx type-coverage --strict --at-least 90
npx type-coverage --detail
```

## dependency-cruiser — Custom arch rules

Declarative rule DSL (`forbidden`/`allowed`/`required`) plus the graph metrics (cycles, instability) the native tools can't compute. The single most useful external for architecture work.

```bash
npx depcruise --no-config --output-type err src/
npx depcruise --no-config --output-type mermaid src/ > deps.md
npx depcruise --no-config --output-type err --affected HEAD src/
```

---

## Python Tools

Use these for Python codebases. `octocode search <path> --pattern/--rule ... --lang py` handles structural Python smells (see [context-ast-pattern-cookbook.md](./context-ast-pattern-cookbook.md)); these tools add linting, type checking, and security scanning on top.

### ruff — Fast Python linter + formatter

Replaces flake8, isort, pyflakes, most pylint rules. `ruff check <target>` · `ruff check --fix <target>` · `ruff check --output-format json <target>` · `ruff format <target>`.

### mypy — Static type checker

Gradual typing for Python. Use when the project has type annotations. `mypy <target>` · `mypy <target> --strict` · `mypy <target> --json-report .`.

### pyright — Fast type checker

Alternative to mypy, often faster. Used by Pylance in VS Code.

```bash
pyright <target>
pyright <target> --outputjson
pyright --verifytypes <package>
```

### bandit — Security linter

Finds common security issues in Python (SQL injection, exec, eval, hardcoded passwords, weak crypto).

```bash
bandit -r <target>
bandit -r <target> -f json
bandit -r <target> -ll   # only medium+ severity
bandit -r <target> -s B101   # skip specific check (e.g. assert)
```

### pytest — Test runner

```bash
pytest --tb=short -q
pytest --cov=<package> --cov-report=term-missing
```

---

## Quick Reference

| Finding | Language | Tool | Command |
|---------|----------|------|---------|
| `dependency-cycle` / `layer-violation` | JS/TS | dep-cruiser | `npx depcruise --no-config -T err <path>` |
| `dead-export` | JS/TS | knip | `npx knip --exports` |
| `unsafe-any` | JS/TS | type-coverage | `npx type-coverage --strict --detail` |
| Lint issues | JS/TS | eslint | `npx eslint <path>` |
| Type errors | JS/TS | tsc | `npx tsc --noEmit` |
| Lint / style | Python | ruff | `ruff check <path>` |
| Type errors | Python | mypy | `mypy <path>` |
| Security issues | Python | bandit | `bandit -r <path>` |
| Test coverage | Python | pytest | `pytest --cov=<pkg> --cov-report=term-missing` |
