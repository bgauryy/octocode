# AST topology immutable-reference parity

The topology harness is a **semantic transport comparison**, not exact raw
parity. It compares the Rust domain result with the data exposed by the
immutable CLI's full JSON transport. Every raw candidate result and complete
reference payload is retained in the machine-readable report so any transport
normalization remains auditable.

Reference artifact:
`/Users/bgaryy/code/octocode/.octocode/implementation/rust-migration/reference/packages/octocode/out/octocode.js`

Candidate artifact:
`/Users/bgaryy/code/octocode/packages/octocode-native/target/debug/octocode`

Native graph behavior is covered by `cargo test --no-default-features`.
The Python Node-differential harness that produced this report has been removed.

Each subprocess has a 30-second timeout and gets a fresh temporary `HOME` and
`OCTOCODE_HOME`. Both implementations receive the same fixed environment:

```text
PATH=/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin
LANG=C.UTF-8
LC_ALL=C.UTF-8
NO_COLOR=1
ALLOWED_PATHS=/Users/bgaryy/code/octocode/packages/octocode-native
ENABLE_LOCAL=true
```

The reference always runs with the explicit supported Node 24 executable. The
fixture and isolated homes use `TemporaryDirectory`; only the evidence report
persists at:

```text
packages/octocode-native/target/parity/ast_graph_report.json
```

The current passing report contains 71 assertions and 71 cases. Each case
stores its query, full raw actual result, full raw expected payload, and the
values compared after transport projection. Continuation traversal has a
2,048-page cycle/budget guard; scan and result-limit expansion have a 32-step
cycle/budget guard.

The projection accounts only for behavior visible in the immutable CLI's full
JSON transport:

- values hoisted into the reference payload's `shared` object are removed from
  candidate result rows when they equal the hoisted value;
- empty arrays and maps omitted by the transport are removed on both sides;
- `why`, `warnings`, and domain `status` are omitted on both sides because this
  reference transport does not expose them in `data`.

The harness does not create reference warnings, continuation explanations,
empty pagination, status fields, or default arrays. These fields were
synthesized by an earlier sensor and therefore could not support exact-parity
claims.

The fixture exercises dependencies, dependents, shortest path, cycles,
reachability, and dead-code analysis over a cyclic, disconnected, polyglot
graph with a diamond and deep chain. It also covers absolute-path root
inference, result and diagnostic pagination, executable continuations, scan
and result-limit expansion, stale diagnostic snapshots, diagnostic pages
outside the result range, typed invalid-query rejection, and both Rust
workspace modes when Cargo metadata has no manifest.

Evidence digests from the passing run:

- Fixture sources: `9fb4ec91dff45dceb3ce3b54a42892ff931ac526c6b30f4b38bbd2635f1e5399`
- Result-page union: `f39cecd3565b846bdfa2f716aec130dd0405fa1d195edcd9d57d4337c51369ac`
- Diagnostic-page union: `f8df8b878a200932db1b12a802a4423f70469bfc14528b8c4d1f24ea37af7ccc`
- Scan-expansion chain: `1b614c25d7a0e55d095373f30e7fdcfe71d357941f7b26b9605fd92c568b950c`
- Limit-expansion chain: `5ca9be451d8f25cf60480c4ed9ba17421d5f858d565a881ac0bcf1097e1707c7`

Explicit exclusions from this matrix:

- Cargo mode with a valid manifest, workspace target/dependency alias mapping,
  conditional dependencies, ambiguous roots, or Cargo process failures.
- JavaScript workspace-package `exports` mapping and explicit
  `package.json` metadata-import leaves.
- Diagnostic streams large enough to reach the hard page-1000 terminal.
- Runtime CLI/MCP dispatch remains pending in the parent integration workstream.

These exclusions are not parity claims.
