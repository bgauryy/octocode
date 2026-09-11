# Terra v3 independent local benchmark harness

This directory is the deterministic infrastructure for the RFC’s S1–S4 gates. It is not
a completed campaign and contains no performance claim. The only accepted corpus is a pair
of detached, byte-locked clones of `langchain-ai/langchain@master` and
`vercel/next.js@canary`. The only accepted agent and judge model is `gpt-5.6-terra`.

Operators should follow [RUNBOOK.md](RUNBOOK.md). The metric and fairness contract is in
[COMPARISON.md](COMPARISON.md). The current public-suite blockers are recorded in
[ORACLE_AUDIT.md](ORACLE_AUDIT.md).

**Current execution status:** the repository can freeze, instrument, validate, and report a
campaign, but it does not bundle the provider-specific controller that creates the complete
`campaign.json`. The Terra role steps in the runbook are therefore an integration contract,
not a turnkey command. Comparative quality, performance, and token results remain unscored.

## Workspace-only candidate

The full preflight runs the repository build commands first, then resolves and hashes the
workspace CLI, native addon, source state (including uncommitted bytes), dependency lock,
and live tool catalog. Every Octocode invocation must begin with:

```text
node <workspace>/packages/octocode/out/octocode.js
```

`npx octocode`, global binaries, package runners, stale outputs, changed catalog bytes, and
scripts outside the receipt’s workspace fail the pairing. Use the combined network path only
when fresh clones are required:

```bash
python3 packages/octocode-benchmark/compare/bin/terra_v3_preflight.py prepare \
  --workspace "$PWD" \
  --corpus-root /isolated/corpus \
  --fixture-manifest packages/octocode-benchmark/compare/terra-v3/suite/public-cases.json \
  --output /isolated/receipts
```

To consume two clones that another trusted workspace-local clone step already pinned, use
`lock-existing`. Both exact SHAs are mandatory; the script verifies each detached `HEAD`,
rejects dirty clones, and hashes all included bytes without executing repository code:

```bash
python3 packages/octocode-benchmark/compare/bin/terra_v3_preflight.py lock-existing \
  --langchain-path /absolute/path/to/langchain \
  --langchain-sha <40-char-sha> \
  --nextjs-path /absolute/path/to/nextjs \
  --nextjs-sha <40-char-sha> \
  --fixture-manifest packages/octocode-benchmark/compare/terra-v3/suite/public-cases.json \
  --output /isolated/receipts/CORPUS.lock.json
```

Receipts are create-once: a differing existing file is never overwritten.

## Fair arm and role contracts

[`contracts/arms.json`](contracts/arms.json) freezes raw ripgrep, raw ast-grep, direct Pyright
and TypeScript LSP, self-hosted Sourcegraph, and workspace Octocode surfaces. Emulation is
invalid. [`contracts/terra-roles.json`](contracts/terra-roles.json) freezes fresh contexts,
`gpt-5.6-terra`, equal per-role reasoning effort across arms, blind X/Y grading, and reversed
confirmation order. `terra_v3_contracts.py` validates these contracts, exact result/patch
graders, Sourcegraph commit convergence, role receipts, and complete three-pass manifests.

The direct semantic control is [`terra_v3_lsp_client.py`](../bin/terra_v3_lsp_client.py), a
minimal stdio JSON-RPC client that initializes the same frozen server used by Octocode,
executes one request, shuts down, and emits the complete response. `terra_v3_arm.py` validates
native CLI argv and tool/workspace/corpus receipts before dispatching the resource wrapper.

## Measurement and trajectory rules

`instrument_command.py` preserves combined stdout/stderr as a unique append-only binary
artifact and appends one v3 JSON record with monotonic wall time, user/system CPU, sampled
process-tree RSS, cgroup-v2 identity/current/peak memory, I/O, page faults, exit/signal,
timeout, context characters, cache cohort, and environment/tool/corpus/workspace receipts.
Linux cgroup v2 is normative; macOS explicitly records unsupported cgroup/I/O sensors and is
diagnostic only. Strict Linux measurements fail closed when required sensors are absent.

`terra_v3_trajectory.py` keeps every attempt and classifies it as `first-valid`,
`schema-invalid`, `runtime-failed`, `productive-success`, `expected-empty`, or
`unproductive-empty`. Empty calls additionally retain one of `expected-absence`,
`scope-empty`, `provider-incomplete`, or `query-miss`. A later repair never deletes or
relabels its schema-invalid or empty predecessor.

## Suite status

[`suite/public-cases.json`](suite/public-cases.json) is frozen at v4 with 20 outcome-oriented
public cases across all six taxonomies. The checked-in receipt materializes 16; semantic
cases p09–p12 remain typed language-server gaps and keep the receipt incomplete.
[`suite/private-manifest.json`](suite/private-manifest.json)
contains 20 hash-only reserved slots across the same taxonomy. It intentionally contains no
private prompt or answer key and remains `slots-reserved`; a separate curator must replace
the envelope digests and mark it `sealed` outside implementation context before any private
campaign is valid. This harness never treats reserved slots as held-out evidence.

## Deterministic checks

```bash
python3 -m unittest discover -s packages/octocode-benchmark/compare/bin -p 'test_*.py'
python3 packages/octocode-benchmark/compare/bin/terra_v3_contracts.py \
  --contracts packages/octocode-benchmark/compare/terra-v3/contracts \
  --suite packages/octocode-benchmark/compare/terra-v3/suite
```

The tests include stable double-clone hashing, tamper and stale-build rejection,
global/published Octocode rejection, exact-set and patch negative controls, mixed-model and
context-reuse rejection, blind-order and three-pass rejection, child/concurrent memory,
failure, timeout, Unicode, and missing-receipt controls. No large repository campaign runs
during unit tests.
