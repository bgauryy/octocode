# Context — AST Pattern Cookbook

AST structural search runs through Octocode structural grep via either transport:

- **CLI:** `octocode grep <path> --pattern '<pattern>' --type ts` · `octocode grep <path> --rule '<yaml>'`
- **MCP:** `localSearchCode({ mode:"structural", pattern:"<pattern>" | rule:{…}, path, langType:"ts" })`

It is **structure-aware** — comments and strings never false-match — and local or clone-backed. For a GitHub repo, use `octocode grep <repoPath> --repo owner/repo --pattern ...`, `octocode clone owner/repo/path`, or `octocode cache fetch`, then run structural grep on the local path.

> The skill no longer ships preset scripts. The "presets" below are plain Octocode structural patterns — copy the pattern into `octocode grep --pattern/--rule` or `localSearchCode(mode:"structural")`. Verify any decision-critical match by reading the `file:line` it returns.

---

## Pattern basics

- **Metavars:** `$X` = one node (captured). `$$$ARGS` = a node list (any arity).
  - `foo($X)` matches `foo(1)` but **not** `foo(1, 2)` — use `foo($$$A)` for any arity.
  - A bare-identifier call doesn't match a member call: `eval($X)` ≠ `window.eval(x)` — use `$F($X)` or `$$.eval($X)`.
- **Give the pattern a literal token** (e.g. `eval`, `console`) — it becomes a text anchor that skips files that can't match. A metavar-only pattern (`$A.$B($C)`) parses every candidate file (slow).
- **Kinds vs patterns:** type annotations are part of the TS AST, so `async function $N($$$P)` misses `async function f(): Promise<void>`. For declarations, match by **node kind** with a `--rule` (`kind: function_declaration`); use `--pattern` for call expressions and shapes where types aren't involved.
- **Relational `--rule` needs `stopBy: end`** on `inside`/`has` sub-rules, or they silently match nothing.

```yaml
# octocode grep src --rule '...'  (YAML)
rule:
  pattern: await $C
  inside:
    kind: for_statement   # TS for-of parses as for_in_statement — check the grammar
    stopBy: end           # REQUIRED
```

---

## JavaScript / TypeScript patterns

| Smell | Pattern (use with `octocode grep <path> --pattern '<pattern>' --type ts`) |
|-------|----------------------------------------------------------------|
| Empty catch | `--rule 'rule:\n  kind: catch_clause\n  not:\n    has:\n      kind: statement_block\n      has: {any: [{kind: expression_statement}, {kind: return_statement}, {kind: throw_statement}]}'` |
| `console.*` left in | `console.$M($$$A)` |
| `console.log` only | `console.log($$$A)` |
| `debugger` | `--rule 'rule: {kind: debugger_statement}'` |
| `eval` | `eval($X)` |
| Explicit `any` | `--rule 'rule: {kind: any}'` (or `cat` + read `: any`) |
| Type assertion `as X` | `$X as $T` |
| Non-null assertion `x!` | `--rule 'rule: {kind: non_null_expression}'` |
| Throwing a string | `throw $S` then filter string literals by reading |
| `Promise.all` (check error handling) | `Promise.all($X)` |
| Default export | `--rule 'rule: {kind: export_statement, has: {field: declaration}}'` (or read `export default`) |
| `import * as X` | `import * as $N from $M` |
| Catch-only-rethrow | `--rule 'rule:\n  kind: catch_clause\n  has: {kind: statement_block, has: {kind: throw_statement}, not: {has: {kind: expression_statement, stopBy: end}}}'` |
| Class declarations | `--rule 'rule: {kind: class_declaration}'` |
| Async functions | `--rule 'rule: {kind: function_declaration, has: {kind: async, field: ...}}'` (or `--type ts` + read) |

For the ones expressed as a kind, the simplest robust form is `octocode grep <path> --rule 'rule: {kind: <node_kind>}' --type ts`. When a kind name is uncertain, get a skeleton with `octocode ls <file>` or `octocode cat <file> --mode symbols`, or match a known snippet and inspect.

---

## Python patterns

Python uses `µ` (micro sign) as the metavariable char in raw patterns (`$` is invalid in Python identifiers); `--rule` handles it automatically. Prefer **kinds** over raw patterns for Python.

| Concept | TS kind | Python kind |
|---------|---------|-------------|
| Function | `function_declaration` | `function_definition` |
| Class | `class_declaration` | `class_definition` |
| Try/catch | `catch_clause` | `except_clause` |
| Import | `import_statement` | `import_statement` / `import_from_statement` |
| Lambda | `arrow_function` | `lambda` |
| Block | `statement_block` | `block` |
| Call | `call_expression` | `call` |

| Smell | Approach |
|-------|----------|
| Bare `except:` | `--rule 'rule: {kind: except_clause, not: {has: {field: value}}}'` |
| `except: pass` | `--rule 'rule: {kind: except_clause, has: {kind: block, has: {kind: pass_statement}}}'` |
| Broad `except Exception` | `except Exception:` (read to confirm) |
| `global` mutation | `--rule 'rule: {kind: global_statement}'` |
| `exec()` / `eval()` | `exec(µX)` / `eval(µX)` |
| `from X import *` | `--rule 'rule: {kind: import_from_statement, has: {kind: wildcard_import}}'` |
| Mutable default arg | read the signature; `--rule` on `default_parameter` with list/dict/set value |
| `print()` in prod | `print(µµµA)` |

---

## Go nuance

A literal-selector pattern like `fmt.Println($X)` matches nothing (a bare snippet is invalid at Go top level, so the structural parser can't parse it). Use a metavar callee `$F($X)`, or a rule with `pattern: { context: "func f(){ fmt.Println($X) }", selector: call_expression }`.

**Grammars supported:** 33 grammars covering ts/tsx, js/jsx, py, go, rs, java, c/h, cpp, cs, bash/sh, html, css/scss/less, scala, json, yaml, toml, ruby, php, kotlin, elixir, hcl, lua, sql, proto, ocaml, zig, r, julia, erlang, swift. Other extensions are skipped silently.

---

## Recommended AST workflow

1. `grep`/`find` (or `localSearchCode`/`localFindFiles`) to narrow candidate files.
2. `octocode grep --pattern/--rule` / `localSearchCode(mode:"structural")` to get structural proof — pair each match with `file:line`.
3. `cat`/LSP for semantic context and blast radius.

This keeps investigation fast and false positives near zero.

---

## Comparing with ast-grep CLI

When cross-checking a structural result or working in an environment without Octocode, the equivalent ast-grep CLI call:

| Octocode | ast-grep |
|---|---|
| `octocode grep src --pattern 'console.log($$$A)' --type ts` | `sg run -p 'console.log($$$A)' -l ts src/` |
| `octocode grep src --rule 'rule: {kind: catch_clause}' --type ts` | `sg run --rule rule.yml src/` (YAML in a file) |
| `octocode grep src --pattern 'eval($X)' --type js --json` | `sg run -p 'eval($X)' -l js src/ --json=stream` |

Key differences:
- Octocode ripgrep-pre-filters on a text anchor before parsing, making it faster on large corpora.
- ast-grep parses every file regardless; use a text anchor pattern (`eval`, `console`) where possible.
- ast-grep `--json=stream` emits one JSON object per line; Octocode `--json` emits the envelope.
- Octocode adds path security, sanitizer, pagination, and YAML output for agent pipelines.
- ast-grep has Dart, Haskell, Nix, Markdown, and Solidity grammars that Octocode doesn't.
- Octocode has SCSS, Less, SQL, Protobuf, OCaml, Zig, R, Julia, Erlang, and TOML that ast-grep doesn't.
