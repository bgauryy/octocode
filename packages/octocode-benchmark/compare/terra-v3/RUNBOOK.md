# Run the Terra v3 benchmark

Use this procedure to compare the current workspace build of Octocode with raw ripgrep,
ast-grep, direct language servers, and self-hosted Sourcegraph over locked LangChain and
Next.js repositories. The measured agent and judge model is `gpt-5.6-terra`.

This is the operator procedure. Read [COMPARISON.md](COMPARISON.md) before interpreting a
result and use [README.md](README.md) for the harness architecture.

## Before you start

Run a publishable campaign only on Linux with:

- Node.js and Yarn versions accepted by the workspace;
- Python 3, Git, raw `rg`, raw `ast-grep`, Pyright, and
  `typescript-language-server`;
- a writable delegated cgroup v2 parent with CPU, memory, and I/O controllers;
- a self-hosted Sourcegraph instance and `src` CLI, with both corpus commits indexed;
- an execution environment that launches fresh `gpt-5.6-terra` contexts and returns
  provider token-usage receipts;
- enough isolated disk and memory for LangChain, Next.js, Sourcegraph, and three passes.

Do not publish measurements from macOS. macOS is useful for unit tests and diagnostic
smoke runs, but it cannot provide the required isolated cgroup sensors.

The independently curated private suite is sealed: the repository contains only opaque case
IDs, signed SHA-256 commitments, and a CMS-encrypted envelope bundle. Curator key material and
plaintext stay outside the repository. This makes the private suite ready for a future run, not
executed and not sufficient to make the overall campaign ready. Public suite v4 is frozen
against the two receipt commits: 16 cases are materialized, while p09–p12 remain typed gaps
until frozen Pyright and TypeScript language servers reproduce their semantic oracles. See
[ORACLE_AUDIT.md](ORACLE_AUDIT.md) and [private/README.md](private/README.md).

## 1. Verify the harness

From the repository root:

```bash
yarn workspace @octocodeai/octocode-benchmark verify
python3 packages/octocode-benchmark/compare/bin/terra_v3_contracts.py \
  --contracts packages/octocode-benchmark/compare/terra-v3/contracts \
  --suite packages/octocode-benchmark/compare/terra-v3/suite
```

This checks deterministic graders, negative controls, arm contracts, role contracts, and
suite shape. It does not make the current private suite campaign-ready.

## 2. Build only the workspace candidate

Development resolution must point at the local packages, including the sibling canonical
core. Do not benchmark `npx octocode`, a global binary, or a published dependency standing
in for a changed workspace package.

The `prepare` command below builds the sibling canonical core, then the native engine,
tools-core, and workspace CLI. Preflight rejects a published/semver canonical-core resolution,
a local resolution aimed anywhere except the sibling source package, stale sibling exports,
or an installed export fingerprint that differs from that build. If local development resolutions are not already
active, follow the repository's `yarn devScript` and `yarn install` development setup before
preflight.

## 3. Create the locked corpus and receipts

Choose new, empty, isolated directories. From the repository root:

```bash
python3 packages/octocode-benchmark/compare/bin/terra_v3_preflight.py prepare \
  --workspace "$PWD" \
  --corpus-root /isolated/terra-v3/corpus \
  --fixture-manifest packages/octocode-benchmark/compare/terra-v3/suite/public-cases.json \
  --output /isolated/terra-v3/receipts
```

This creates detached LangChain and Next.js clones and freezes
`WORKSPACE.receipt.json` and `CORPUS.lock.json`. Receipts are create-once and are not
silently replaced.

To use two existing trusted clones instead, call `lock-existing` with both absolute paths
and exact 40-character commit IDs. Do not execute code from either benchmark repository
during corpus locking.

## 4. Reverify immediately before measurement

```bash
python3 packages/octocode-benchmark/compare/bin/terra_v3_preflight.py verify \
  --workspace "$PWD" \
  --corpus-root /isolated/terra-v3/corpus \
  --workspace-receipt /isolated/terra-v3/receipts/WORKSPACE.receipt.json \
  --corpus-lock /isolated/terra-v3/receipts/CORPUS.lock.json
```

Any source, dependency, CLI, native-addon, tool-catalog, schema, corpus, or fixture drift
invalidates the receipt. Rebuild and start a new campaign rather than modifying an existing
receipt.

## 5. Sourcegraph and cache cohorts

Index both repositories at the exact commits in `CORPUS.lock.json`. Record the Sourcegraph
instance digest, repository names, indexed commits, and convergence status in a separate
receipt.

Prepare cold, warm-process, and warm-index trials independently. A cohort name is not proof
that its state was prepared correctly; the campaign controller must perform and record the
cache transition. Measure index construction and incremental refresh separately from query
latency.

## 6. Validate each native arm before a trial

Use `terra_v3_arm.py --dry-run` with the complete trial arguments. For example, a ripgrep
trial uses raw `rg --json`; an Octocode trial must begin with
`node packages/octocode/out/octocode.js`. The common command shape is:

```text
python3 packages/octocode-benchmark/compare/bin/terra_v3_arm.py
  --arm <arm>
  --workspace <workspace>
  --workspace-receipt <WORKSPACE.receipt.json>
  --corpus-lock <CORPUS.lock.json>
  --contracts <contracts-directory>
  --fixture-manifest <public-cases.json>
  --log <measurements.jsonl>
  --artifact-dir <artifact-directory>
  --label <case:arm:pass:attempt>
  --cache-cohort <cold|warm-process|warm-index>
  --logical-call-id <case:arm:pass>
  --attempt-index <positive-integer>
  --cgroup-parent <delegated-cgroup-v2-parent>
  [--empty-classification <classification>]
  [--sourcegraph-receipt <receipt>]
  [--language-server-receipt <receipt>]
  [--dry-run]
  -- <native executable and arguments>
```

Before semantic trials, create an immutable resolved-server receipt with
`terra_v3_lsp_receipt.py`. It launches the resolved server, records initialization
capabilities and a readiness probe, and binds executable/package digests, configuration,
and the exact repository workspace root. Semantic trials additionally provide that receipt,
`--lsp-root`, `--lsp-method`, and `--lsp-params-json`. After the separator, replay the
receipt's absolute `resolvedCommand` exactly. Changed arguments, a bare executable lookup,
changed config/package bytes, a different workspace, or an unready server fails preflight.

Every measured arm rehashes the exact corpus bytes and validates workspace, fixture,
contract, tool, and Sourcegraph receipts before starting the resource sensor.

## 7. Terra roles

This repository does not include the provider-specific campaign controller that
creates `campaign.json`. The following is the required adapter contract. The controller must
launch fresh Terra contexts and export native provider usage receipts; the local harness
cannot infer those receipts or substitute character counts.

For every frozen case, eligible arm, cache cohort, and pass 1–3:

1. Start a fresh `gpt-5.6-terra` runner context at the contract's reasoning effort.
2. Give it only the outcome prompt, assigned native-tool primer, frozen budget, and corpus
   receipt.
3. Execute each native call through `terra_v3_arm.py`. Preserve every failed schema attempt,
   runtime failure, expected empty result, unproductive empty result, and raw output artifact.
4. Store provider-reported input, cached-input, output, reasoning, and total tokens. Never
   estimate tokens from characters.
5. After both paired answers exist, start a fresh blind Terra judge with X/Y labels.
6. Run the reversed-order confirmation judge required by the contract.
7. Reveal X/Y-to-arm assignments only in the post-run assignment records, cryptographically
   bound to the answer digests.

The repository validates these role receipts but does not contain credentials or a provider
specific Terra launcher. The campaign controller must export its native provider usage
receipt into the validated campaign format.

## 8. Validate the complete campaign

From `packages/octocode-benchmark`:

```bash
python3 compare/bin/terra_v3_contracts.py \
  --contracts compare/terra-v3/contracts \
  --suite compare/terra-v3/suite \
  --campaign /isolated/terra-v3/run/campaign.json
```

Validation requires the exact lane-derived arm matrix, all three passes, tool and source
receipts, raw and summarized trajectories, provider token usage, blind quality scores,
deterministic anchor receipts, Terra role receipts, and reversed confirmation assignments.
Unknown, missing, duplicated, or extra runs fail validation.

## 9. Produce the comparison artifact

Only after campaign validation succeeds:

```bash
python3 compare/bin/terra_v3_report.py \
  --campaign /isolated/terra-v3/run/campaign.json \
  --contracts compare/terra-v3/contracts \
  --suite compare/terra-v3/suite \
  --output /isolated/terra-v3/run/comparison.json
```

The report keeps quality, runtime performance, provider tokens, context characters, and
trajectory efficiency separate. Pairwise ratios use only matching case, pass, and cache
cohort records. Runner tokens compare arms; judge and confirmation tokens are shared
campaign overhead and must not be folded into arm ratios.

## Failure rules

- Exit status 125 from strict instrumentation means the required sensor was unavailable;
  discard the measurement.
- A missing binary, unindexed Sourcegraph commit, stale build, changed receipt, malformed
  result count, or unavailable cgroup is a blocked trial—not a loss for that arm.
- If an oracle or grader is wrong, stop the campaign, version the suite, rematerialize every
  affected receipt, and start again.
- Public results are orientation. Only the independently sealed private suite may gate a
  release.
