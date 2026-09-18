# Local-tool removal held-out eval

This deterministic eval freezes the post-migration local-tool surface and
checks behavior through the built CLI. The original remote routing fixture and
grader remain in place; this local eval is an additional independent gate. It
covers the four public local tools:
`localSearch`, `astSearch`, `localFetch`, and `lspSearch`.

## Goal and decision rule

The goal is to remove `localAnalyzeGraph` and `lspGetSemantics` while retaining
the behavior they provided through `astSearch` topology and `lspSearch`.

The local fixture is
[`fixtures/local-tool-removal-held-out.json`](../fixtures/local-tool-removal-held-out.json).
It is held out from implementation work and records a fixed contract:

- all four local public names are present;
- both retired names reject as unknown tools;
- all six topology algorithms remain callable with their graph parameters;
- all nine `localSearch` result views and three regex modes execute;
- all three exact-content views execute, with Unicode and CRLF preserved;
- returned continuation queries are schema-valid and representative page unions
  are executed;
- no legacy export or runner module remains in the native runtime source or built output;
- anchored LSP is attempted and is skipped only when its provider is absent or
  not ready.

The benchmark does not claim a speedup. It records duration as diagnostic data
only. Acceptance requires every deterministic behavior and removal guardrail to
pass; provider-dependent LSP is reported separately when unavailable.

## Runnable harness

Run after building the affected packages and CLI:

```bash
node --import tsx \
  .octocode/octocode-eval-benchmark/2026-09-09-local-tools-removal/run-removal-benchmark.mjs
```

The harness uses a temporary three-file TypeScript fixture, invokes the actual
`packages/octocode/out/octocode.js` CLI, validates every returned continuation
against the live direct-tool schemas, and writes:

- `.octocode/octocode-eval-benchmark/2026-09-09-local-tools-removal/result.json`
- `.octocode/octocode-eval-benchmark/2026-09-09-local-tools-removal/REPORT.md`

The report includes pass/fail/skip counts, exact commands, observations, and
the real-versus-provider-dependent limitations. The temporary fixture is
synthetic; the CLI calls and schema checks are real.

For repeated frozen trials, set the OCTOCODE_REMOVAL_ROUND environment variable
to 1, 2, or 3; each run writes a separate result-round-N.json and
REPORT-round-N.md receipt.

## Package regression gate

The package test checks the frozen public names, retired-name absence, route
schema compatibility, and coverage guardrails:

```bash
yarn workspace @octocodeai/octocode-benchmark test
yarn workspace @octocodeai/octocode-benchmark lint
yarn workspace @octocodeai/octocode-benchmark typecheck
```

This package test is a contract check. The runnable harness is the acceptance
sensor because it executes the built CLI and its continuations.
