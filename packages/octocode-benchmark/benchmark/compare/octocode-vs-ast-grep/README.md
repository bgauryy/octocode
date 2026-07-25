# Octocode vs `ast-grep` — Structural Search Benchmark

10 tasks. Two lanes: **parity** (Q1–Q5, where both tools apply — Octocode's AST
results must match `ast-grep` exactly) and **beyond-AST** (Q6–Q10, where the task
needs capabilities `ast-grep` doesn't have: semantic identity, remote repos,
text+structural combos, minified reading).

- **Arm A (`ast-grep`)**: ONLY the `ast-grep` CLI (`ast-grep run -p '<pattern>'`,
  `ast-grep scan` with YAML rules). Local files only.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js`
  (`localSearchCode mode:"structural"`, plus its other surfaces).

Both arms run against the **same local checkout** — this repo (deterministic, no
network) for Q1–Q6, Q8–Q10; a pinned public repo for Q7 (remote-as-local).

| Q | Lane | Tests |
|---|---|---|
| Q1 | parity | Simple metavar pattern — identical match count |
| Q2 | parity | Function pattern with body metavar (`$$$`) |
| Q3 | parity | Relational YAML rule (`inside`/`has`) |
| Q4 | parity | Method-call shape `$OBJ.$M($$$)` |
| Q5 | parity | Same pattern across two languages (TS + Rust) |
| Q6 | beyond | Semantic callers of a function (LSP, not text/AST shape) |
| Q7 | beyond | Structural search on a GitHub repo without a manual clone |
| Q8 | beyond | Files matching a regex AND a structural shape |
| Q9 | beyond | Read the enclosing function of a match, minified/outlined |
| Q10 | beyond | Match in a non-`ast-grep`-first format / broad language matrix |

## Why these

`ast-grep` is excellent at one thing: local AST structural matching. Q1–Q5 verify
Octocode is **at least as correct** on that turf (the historical result:
structural match correctness ties with zero count differences). Q6–Q10 show the
surface `ast-grep` lacks: `lspGetSemantics` resolves real call graphs (not just
call *shapes*); Octocode can materialize a remote repo and run AST on it;
`localSearchCode` combines text+structural filters; `localGetFileContent
minify:"symbols"` turns a match into a cheap function outline; and Octocode's
format matrix spans far more than `ast-grep`'s grammar set.

## Oracle status

- **Q1–Q5 (parity):** the oracle is **cross-tool agreement** — both tools must
  return the **same match count** on the same checkout. This self-verifies at run
  time; a nonzero count difference is the finding (record which tool differed).
- **Q6–Q10 (beyond):** oracle is the semantic answer (verify at run time on the
  frozen SHA). `ast-grep`'s expected ceiling is noted per question — it can find
  syntactic candidates but cannot complete the semantic/remote/read step.

Shared method + metrics: [`../README.md`](../README.md).
