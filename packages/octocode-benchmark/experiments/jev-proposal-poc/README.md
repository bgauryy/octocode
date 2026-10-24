# Archived JEV development experiments

This folder preserves the fixtures for the completed October 14, 2026 JEV studies.
Their runnable prototypes were retired when the tool contract removed public physical
paging controls, source snapshots and the old ranking requests. Rewriting those runners
against today's contract would change the experiment; it would not reproduce the frozen
results. Historical results, source fixtures and offline summary programs remain available.

The studies compared source-order reads, classifier-guided reads, atomic questions,
verification context, and instruction/chunk boundaries. They used exposed development
fixtures and measured retrospective evidence arrival and serialized exchanges. They did
not run an autonomous host agent or establish whole-agent savings. Failed attempts,
unknown usage, exact source reconstruction and paired per-case ratios remain part of the
record; mock usage must not be reported as inference usage.

| Study | Recorded evidence | Offline summary |
|---|---|---|
| Initial context proposal | [Development report](../../../../.octocode/octocode-eval-benchmark/2026-10-14-jev-proposal-poc/README.md) | `scripts/jev_proposal_poc_summary.mjs` |
| Atomic questions and verification reads | [Atomic-workflow report](../../../../.octocode/octocode-eval-benchmark/2026-10-14-jev-atomic-workflows/README.md) | `scripts/jev_question_poc_summary.py` |
| Local/GitHub search and read transfer | [Four-tool report](../../../../.octocode/octocode-eval-benchmark/2026-10-14-jev-four-tools/README.md) | `scripts/jev_four_tools_poc_summary.py` |
| Instruction and chunk boundaries | [Instruction/chunk report](../../../../.octocode/octocode-eval-benchmark/2026-10-14-jev-instructions-chunks/README.md) | `scripts/jev_chunk_instruction_poc_summary.py` |

The [original RFC](../../../../.octocode/octocode-rfc-generator/2026-10-14-jev-goals-and-evidence/RFC.md)
records the proposed design and acceptance criteria at that time. Its recorded conditions
are historical, not current tool instructions.

Run an offline summary from the repository root with the corresponding saved `run.json`:

```sh
node packages/octocode-benchmark/scripts/jev_proposal_poc_summary.mjs /path/to/saved/run.json
python3 packages/octocode-benchmark/scripts/jev_question_poc_summary.py /path/to/saved/run.json
python3 packages/octocode-benchmark/scripts/jev_four_tools_poc_summary.py /path/to/saved/run.json
python3 packages/octocode-benchmark/scripts/jev_chunk_instruction_poc_summary.py /path/to/saved/run.json
node --test packages/octocode-benchmark/scripts/jev_proposal_poc_summary.test.mjs
```

The token summaries require `tiktoken==0.14.0` and record both `o200k_base` and
`cl100k_base`. Counts cover saved CLI output and serialized arguments, excluding host
framing, schema loading, history replay, reasoning and final answers. They are not
observed host billing.

For a new comparison, freeze new conditions and use the current
[local benchmark cookbook](../../local/README.md) and [evaluation method](../../README.md#choose-the-benchmark-and-evaluation-method).
Current source traversal is tested by the [end-to-end harness](../../local/e2e/README.md).
Do not pool new automatic-pagination results with these older fixed-page policies.
