# Output

BLUF: correctness-first verdict, measured character cost, deciding evidence and limitations. Use [REPORT_TEMPLATE.md](references/REPORT_TEMPLATE.md) and [paired statistics](references/aggregation-and-stats.md); retain failures and unresolved questions in the reported counts.

## Saved result

New runs go under `<workspace>/.octocode/octocode-benchmark/<YYYY-MM-DD>-<slug>/`: `report.md` is the entry point, `tmp/campaign/` holds complete instrumented logs, answers, blind packets and judge verdicts. Choose a fresh slug for a new run; preserve existing campaigns and published `results/` unless updating them was requested. The workspace is the repository root, not the installed skill or benchmark package directory.

Record actual versions and source revisions where they affect applicability. Preserve exact Unicode measurements, commands, tool outputs and final answers in their instrumented records. Link those files from the report; a brief or chart never replaces them.

`per_question_summary.py` currently reads fixed historical campaign names from its source. Use it only to reproduce that historical rollup; it does not measure a newly created campaign. For a new run, validate its explicit campaign path and apply the paired calculations in the statistics guide.

## HTML delivery

Deliver Markdown first. If HTML is requested, use an available report renderer on the saved report, keep its source and records intact, and verify the page before claiming visual success. Discover the renderer before promising it; this package skill does not bundle one.
