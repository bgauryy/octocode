# Benchmark archive locations

All benchmark plans, question sets, reports and retained campaign evidence belong under
`packages/octocode-benchmark`. On 2026-11-14 the inactive skill-output benchmark archives below
were moved here with same-filesystem renames. Raw request/answer bodies were preserved; their
embedded historical absolute paths still describe the original execution. They are provenance,
not current runnable paths. No compatibility symlink or second copy was left at the old location.

| Previous repository-relative location | Current package-relative location |
|---|---|
| `.octocode/octocode-benchmark` | `campaigns/legacy/octocode-benchmark` |
| `.octocode/octocode-eval-benchmark` | `campaigns/legacy/octocode-eval-benchmark` |
| `.octocode/benchmark-failures-2026-10-18` | `campaigns/legacy/benchmark-failures-2026-10-18` |
| `.octocode/clasify-bench` | `campaigns/legacy/clasify-bench` |
| `.octocode/omp-comparison` | `campaigns/legacy/omp-comparison` |
| `.octocode/octocode-rating` | `campaigns/legacy/octocode-rating` |
| `.octocode/evals` | `campaigns/legacy/evals` |
| `.octocode/octocode-workflows/tmp/2026-11-14-final-verification` | `campaigns/20.2.0-2026-11-14-final-verification` |
| `.octocode/octocode-workflows/tmp/2026-11-14-advanced-comparison` | `campaigns/20.2.0-2026-11-14-advanced/orchestration` |
| `.octocode/octocode-dev/final-verification-2026-11-14.md` | `results/rounds/20.2.0-2026-11-14-final-verification/REPORT.md` |
| `.octocode/octocode-graph-dev/2026-11-12-battery` | `campaigns/legacy/2026-11-12-graph-battery` |
| `.octocode/octocode-workflows/local-tools-rating` | `campaigns/legacy/local-tools-rating` |
| `.octocode/octocode-workflows/release-benchmark-2026-10-30` | `campaigns/legacy/release-benchmark-2026-10-30` |
| `.octocode/octocode-dev/browser-workflow-ratings-2026-10-23.md` | `campaigns/legacy/browser-workflow-ratings-2026-10-23.md` |

The migration retained directory/file identities; 56 symlink targets were adjusted to keep
pointing to their relocated or original destinations. Receipts: [initial moves](campaigns/20.2.0-2026-11-14-tool-ratings/migration-receipt.json),
[additional moves](campaigns/20.2.0-2026-11-14-tool-ratings/migration-additional.json).

Source-owned unit tests, runtime microbenchmarks and non-benchmark debugging artifacts stay with
their owning packages. New behavioral benchmark runs save data in `campaigns/<version>-<date>-<purpose>/`,
tracked conclusions in `results/rounds/<version>-<date>-<purpose>/`, and reusable questions in `questions/`.
