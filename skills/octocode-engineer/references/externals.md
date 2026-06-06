# External Tools

**Ask user before running.** Use `npx` (JS/TS) or `pip`/`pipx` (Python) — avoid global installs.

## Scanner Already Covers

| Domain | Categories | Flags |
|--------|-----------|-------|
| Duplicates | `duplicate-function-body`, `similar-function-body`, `duplicate-flow-structure` | `--similarity-threshold 0.8` |
| Unused deps | `unused-npm-dependency` | `--features=dead-code` |
| Dead exports | `dead-export`, `dead-re-export`, `semantic-dead-export` | `--features=dead-code --semantic` |
| Python smells | 13 `py-*` AST presets (bare-except, mutable-default, eval, etc.) | `scripts/ast/search.js --preset py-*` |

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

Use when project has CSS files. Scanner only handles JS/TS.

```bash
npx stylelint "**/*.css"
npx stylelint "**/*.scss"
npx stylelint --fix "**/*.css"
npx stylelint --formatter json "**/*.css"
```

## knip — Framework-aware dead code

100+ plugins (Next.js, Remix, Angular) detect framework-specific usage scanner can't see.

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

Project-wide typed-vs-any ratio. Scanner counts per-file `any`; this gives one number.

```bash
npx type-coverage
npx type-coverage --strict --at-least 90
npx type-coverage --detail
```

## dependency-cruiser — Custom arch rules

Declarative rule DSL (`forbidden`/`allowed`/`required`). Scanner has 28 built-in detectors; this adds project-specific constraints.

```bash
npx depcruise --no-config --output-type err src/
npx depcruise --no-config --output-type mermaid src/ > deps.md
npx depcruise --no-config --output-type err --affected HEAD src/
```

---

## Python Tools

Use these for Python codebases. The scanner handles Python AST analysis (complexity, duplication, presets), but these tools cover linting, type checking, and security that the scanner does not.

### ruff — Fast Python linter + formatter

Replaces flake8, isort, pyflakes, and most pylint rules. Very fast (~100x faster than pylint).

```bash
ruff check <target>
ruff check --fix <target>
ruff check --output-format json <target>
ruff format <target>
ruff format --check <target>
```

### mypy — Static type checker

Gradual typing for Python. Use when the project has type annotations.

```bash
mypy <target>
mypy <target> --strict
mypy <target> --show-error-codes --no-error-summary
mypy <target> --json-report .
```

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

### JavaScript / TypeScript

| Finding | Tool | Command |
|---------|------|---------|
| `dependency-cycle` | dep-cruiser | `npx depcruise --no-config -T err <path>` |
| `dead-export` | knip | `npx knip --exports` |
| `unsafe-any` | type-coverage | `npx type-coverage --strict --detail` |
| `layer-violation` | dep-cruiser | `npx depcruise --no-config -T err <path>` |
| Lint issues | eslint | `npx eslint <path>` |
| Type errors | tsc | `npx tsc --noEmit` |
| CSS issues | stylelint | `npx stylelint "**/*.css"` |

### Python

| Finding | Tool | Command |
|---------|------|---------|
| Lint / style | ruff | `ruff check <path>` |
| Type errors | mypy | `mypy <path>` |
| Type errors (fast) | pyright | `pyright <path>` |
| Security issues | bandit | `bandit -r <path>` |
| Test coverage | pytest | `pytest --cov=<pkg> --cov-report=term-missing` |
| Dead code (imports) | ruff | `ruff check --select F401 <path>` |
