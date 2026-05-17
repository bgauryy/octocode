<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="320" alt="Octocode Logo">
  <h1>Octocode Code Engineer</h1>
  <p><strong>AI agent skill for safe, codebase-aware engineering — JavaScript/TypeScript and Python</strong></p>
</div>

## Why use this skill
Use this skill when you want an agent to change code without guessing.

It helps you:
- find existing implementations before adding new code
- estimate blast radius before refactors and renames
- detect architecture, quality, dead-code, security, and test issues
- validate findings with local + LSP evidence before reporting
- track improvement with file-level findings and hybrid quality ratings
- analyze Python codebases with AST structural search and complexity metrics

## What it does
The skill runs a scan + validation workflow:
1. Map structure: files, functions, flows, dependency graph.
2. Detect issues: 80+ categories across architecture, quality, dead code, security, and test quality.
3. Validate critical findings: use local search + LSP navigation instead of raw heuristics only.
4. Present results: structured findings with evidence chain, confidence level, impact assessment, and concrete next steps ([output format](./references/output-format.md)).

## Language support

| Language | AST search | Scanner (complexity, duplication) | Dependency graph | Semantic analysis |
|----------|-----------|----------------------------------|-----------------|-------------------|
| TypeScript / JavaScript | 22 presets | Full (all 80+ categories) | Full | Full (`--semantic`) |
| Python | 13 `py-*` presets | Complexity, duplication, nesting, god-function | Not yet | Not yet |

## Key features
- **Architecture analysis**: coupling, cycles, chokepoints, dependency critical paths.
- **Code quality analysis**: complexity, duplication, risky async patterns, error-boundary gaps.
- **Dead-code hygiene**: dead exports/files, unused deps, barrel issues.
- **Security checks**: secrets, injection risks, traversal risks, unsafe sinks.
- **Test quality checks**: assertion density, mocking hygiene, cleanup issues.
- **AST tools**: 35 structural search presets (22 JS/TS + 13 Python) and AST tree exploration.
- **Hybrid quality ratings** (AI + structure): Architecture & Structure, Folder Topology/Structure Health, Naming Quality, Common/Shared Layer Health, Maintainability & Evolvability, Codebase Consistency.
- **Output format template**: standardized finding presentation with evidence chains, confidence levels, and impact assessment.

## Rating model behavior
- Soft-signal scoring (not rigid pass/fail lint rules).
- Test files are excluded from hybrid ratings unless `--include-tests` is enabled.
- Generated/minified/vendor paths are excluded from hybrid ratings by default.
- Advisory categories are downweighted relative to hard defects to reduce noise.

## Requirements
For full capability, run with Octocode MCP local tools enabled:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "type": "stdio",
      "args": ["octocode-mcp@latest"],
      "env": {
        "ENABLE_LOCAL": "true"
      }
    }
  }
}
```

Without local tools, scanning still works, but semantic validation is reduced.

## Install
```bash
npx octocode-cli skills install --skill octocode-code-engineer
```

Multi-target install:
```bash
npx octocode-cli skills install --skill octocode-code-engineer --targets claude-code,claude-desktop,cursor,codex,opencode
```

## Common commands
From `skills/octocode-code-engineer/`:

```bash
# Fast default scan (JS/TS or Python)
node scripts/run.js --root /path/to/repo --out .octocode/scan/latest

# Include graph + semantic signals
node scripts/run.js --root /path/to/repo --out .octocode/scan/latest --graph --semantic --flow

# Analyze with tests
node scripts/run.js --root /path/to/repo --out .octocode/scan/latest --include-tests

# AST search — Python presets
node scripts/ast/search.js --preset py-bare-except --root /path/to/python/project
node scripts/ast/search.js --preset py-mutable-default --root /path/to/python/project --json

# List all presets (JS/TS + Python)
node scripts/ast/search.js --list-presets
```

## Output files
Typical outputs in `.octocode/scan/<run>/`:
- `summary.md`: concise human report and triage guidance
- `summary.json`: machine-readable overview + hybrid ratings
- `findings.json`: all findings with category/severity/location
- `architecture.json`, `code-quality.json`, `dead-code.json` (+ optional `security.json`, `test-quality.json`)
- `file-inventory.json`: per-file functions/flows/dependencies
- optional `graph.md`, `ast-trees.txt`

## Reference docs
- [SKILL.md](./SKILL.md) — full agent instructions
- [AST reference](./references/ast-reference.md) — presets, pattern syntax, Python node kinds
- [CLI reference](./references/cli-reference.md) — all scanner flags and thresholds
- [Output format](./references/output-format.md) — how to present findings to users
- [Tool workflows](./references/tool-workflows.md) — 21 investigation workflows
- [Quality indicators](./references/quality-indicators.md) — full detector catalog
- [External tools](./references/externals.md) — eslint, tsc, ruff, mypy, pyright, bandit, knip

## When not to use this skill
Use other tools for:
- syntax/type errors (`tsc`, `mypy`, `pyright`)
- style formatting/lint policy (ESLint/Prettier, ruff)
- runtime debugging (tests/debugger, pytest)

## License
MIT License © 2026 Octocode — see [LICENSE](https://github.com/bgauryy/octocode-mcp/blob/main/LICENSE).
