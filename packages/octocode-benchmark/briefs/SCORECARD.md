# Scorecard template

One round fills in one copy of this template and saves it as
`results/rounds/<version>-<YYYY-MM-DD>/SCORECARD.md`, then adds the round to the version timeline in the
[results index](../results/README.md). Keep the headings; replace every `…`. Write "not checked" rather than
leaving a cell empty, and never carry a rating over from an earlier round. How to check each area is in
[CHECKS.md](../CHECKS.md); how to run and compare the benchmark is in [METHOD.md](../METHOD.md).

---

# Scorecard: Octocode `<version>`, `<YYYY-MM-DD>`

| Round | Value |
|---|---|
| Version | `octocode --version` output |
| Binary | path and SHA-256 of the native binary checked |
| Source | commit, and whether the tree was dirty |
| Previous version compared | version, and the round or baseline it came from |
| Surfaces | CLI, MCP, or both |
| Agents and models | checking agent; runner and judge models for benchmark rows |
| Checked by / signed off by | the signer is not the fixer of any defect below |

## Verdict

Ready means every check in the area passed or has an accepted, stated exception. One blocking defect makes the
area Not ready. Core tools come first: without them the round is Not ready overall.

| Area | Verdict | Blocking items (defect IDs) |
|---|---|---|
| Core tools | Ready / Not ready | … |
| Security | Ready / Not ready | … |
| Skills | Ready / Not ready | … |
| Release gates | Ready / Not ready | … |
| Benchmark | Better / Same / Worse than the previous version | … |
| **Overall** | **Ready / Not ready** | … |

## Core tools

Ratings are **1–10**, using [RATINGS.md](../RATINGS.md). Do not convert historical 1–5
ratings. Name the question IDs, exact build, measured weight and confidence. Every deduction
needs an observed cost, limitation or defect; a good tool may receive a good score.

| Tool | Correctness | Schema/input | Output | Pagination | Workflow | Efficiency | Cache/freshness | Overall | Confidence / weight | Question IDs / evidence |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| localSearch | … | … | … | … | … | … | … | … | … | … |
| localFetch | … | … | … | … | … | … | … | … | … | … |
| structureSearch | … | … | … | … | … | … | … | … | … | … |
| astSearch | … | … | … | … | … | … | … | … | … | … |
| lspSearch | … | … | … | … | … | … | … | … | … | … |
| graphBuild | … | … | … | … | … | … | … | … | … | … |
| graphQuery | … | … | … | … | … | … | … | … | … | … |
| clasify | … | … | … | … | … | … | … | … | … | … |
| ghSearchRepo | … | … | … | … | … | … | … | … | … | … |
| ghSearchCode | … | … | … | … | … | … | … | … | … | … |
| ghStructure | … | … | … | … | … | … | … | … | … | … |
| ghGetFileContent | … | … | … | … | … | … | … | … | … | … |
| ghSearchHistory | … | … | … | … | … | … | … | … | … | … |
| ghGetHistoryItem | … | … | … | … | … | … | … | … | … | … |
| artifactSearch | … | … | … | … | … | … | … | … | … | … |
| securityCheck | … | … | … | … | … | … | … | … | … | … |
| ghCloneRepo | … | … | … | … | … | … | … | … | … | … |

Below 8, explain the practical weakness. Use NA only for an inapplicable dimension and
unverified for missing evidence; the latter makes the overall rating provisional. Publish
CLI and MCP coverage, actual valid/invalid outputs, complete-page reconstruction, first/repeat
times, Unicode characters and bytes, both output streams and the lean rival commands.

Workflows ([shared checks](../CHECKS.md)):

| Workflow | Calls this round | Calls last round | Answer right? | Note |
|---|---|---|---|---|
| … | … | … | … | … |

## Security

| Check ([list](../CHECKS.md#security)) | Pass / Fail / not checked | Evidence or defect ID |
|---|---|---|
| Partial redaction | … | … |
| Large-file masking | … | … |
| Credential files withheld | … | … |
| Symlinks | … | … |
| Path policy | … | … |
| Provider egress | … | … |
| Tokens and refusals | … | … |
| Clone sandbox | … | … |
| Prompt injection | … | … |
| Processes | … | … |

## Skills

Ratings 1–5 ([checks](../CHECKS.md#skills)). Activation: own requests hit / total, neighbour requests wrongly hit / total.

| Skill | Activation | Own steps | Tool use | Output contract | Links | Defects |
|---|---|---|---|---|---|---|
| octocode-… | … | … | … | … | … | … |

## Release gates

| Gate | Command | Exit | Note |
|---|---|---|---|
| Lint | `node skills-dev/octocode-dev/scripts/dev.mjs lint:ci` | … | … |
| Typecheck | `… typecheck:ci` | … | … |
| Tests | `… test:ci` | … | … |
| Docs | `… docs:verify` | … | … |
| Skills | `… skills:check` | … | … |
| Native | the native package's checks per the [release guide](../../../skills-dev/octocode-dev/docs/RELEASE.md) | … | … |
| Areas owned elsewhere | browser, graph battery, agents communication, packaged credentials ([Areas](../CHECKS.md#areas)) | … | … |

## Benchmark

Against the previous version and the stored baselines, per [METHOD.md](../METHOD.md#compare-a-new-version).
Ratios are this version ÷ the other, per question, geometric mean with 95% interval; below 1× means this version
used less.

| Comparison | Question set (IDs) | Correctness this / other | More correct by > 1 point: this / other / tied | Characters ratio (95% CI) | Leaner on | Time ratio | Verdict |
|---|---|---|---|---|---|---|---|
| vs previous version | … | … | … | … | … | … | Accept / Reject / Regression / Inconclusive / Invalid |
| vs gh-lean (stored answers, blind) | … | … | … | … | … | not measured | … |
| vs plain gh, RTK, Headroom (stored numbers) | … | … | … | … | … | not measured | … |
| vs lean shell (local) | … | … | … | … | … | … | … |
| Capability rounds (rg, ast-grep, fd, sed, registries, OSV, graphify) | tasks | rows right on both | Octocode / expert / even | … | … | … | … |

## Defects

| ID | Area and tool | Severity | Repro call (exact, on the pinned binary) | Expected | Observed | Owner |
|---|---|---|---|---|---|---|
| D1 | … | blocker / major / minor | … | … | … | … |

Severity: **blocker** makes its area Not ready (wrong answer, lost evidence, unreachable page, secret shown, a
hang); **major** degrades an answer or costs an agent extra calls; **minor** wording or a lean-output miss.

## Not checked

Every area, tool, surface, dimension or question that this round skipped, and why.

- …
