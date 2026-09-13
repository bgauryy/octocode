# Local and GitHub research diagnostic

Compare fresh Octocode and raw-tools agents on the pinned LangChain and Next.js
sources. [QUESTIONS.md](QUESTIONS.md) defines the tasks; [RUBRIC.md](RUBRIC.md)
contains ten cited behavioral checks for A01/A02. A03–A12 remain unscored candidates.
This public diagnostic does not replace the [Terra v3 gate](../terra-v3/README.md).

## Outcomes and evidence

The [tool-quality audit](../../../../.octocode/octocode-eval-benchmark/tool-quality-20260913/REPORT.md)
covers current schemas, result contracts, local semantics, rewrite application and
recovery, GitHub tools, and all eight package ecosystems. Its supplied-evidence
completeness experiment is separate from end-to-end agent research.

Earlier [API efficiency](../../../../.octocode/octocode-eval-benchmark/github-api-efficiency-20260913/REPORT.md)
and [all-tool agent evaluation](../../../../.octocode/octocode-eval-benchmark/all-tools-20260913/REPORT.md)
reports retain their frozen questions, controls, raw receipts, exclusions, and grades.
Do not pool or rescore experiments with different prompts, builds, or controls.
No general winner is established; Sourcegraph has not been compared.

## Prepare and check

Run from the workspace root after rebuilding the measured packages. Supply exact
clone roots using `--langchain /ABS/CLONE --nextjs /ABS/CLONE`, or reuse the `corpora`
records from an existing frozen `preflight.json`. No branch alias or host-specific
clone suffix is guessed. The pinned commits remain:

- LangChain: `67ee6cb63dd9ae7f3a4dfedc3095652bce15a125`
- Next.js: `d155ba9ebfffe4742efefda8d68c2e0e8e490924`

Print the resolved plan without creating artifacts or launching a model:

```bash
python3 packages/octocode-benchmark/compare/advanced-research-v1/pilot.py \
  --corpus-receipt .octocode/octocode-eval-benchmark/all-tools-20260913/eval/preflight-v6/preflight.json \
  --passes 3 --cases A01 A02 \
  --output-dir .octocode/octocode-eval-benchmark/local-research-next
```

Add `--check` and choose a separate new output directory to verify the actual
commits, clean corpus state, runtime fingerprints, installed commands, live catalog,
the verbatim core context, and selected tool schemas. It writes sealed preflight/catalog/schema/report
receipts without model trials. An existing output directory is never reused;
artifacts cannot be placed inside a measured corpus.

## Run the controlled pilot

The current runner uses `recoverable-read-surfaces-v9`. It freezes and delivers the
measured CLI's verbatim `context --compact`, together with the catalog and selected
schemas; Octocode research examples are owned by public core.

The observer accepts documented read-only `gh api -H/--header` and `-q/--jq` forms with
bounded representation/version headers, while retaining repository, commit and
GET-only restrictions. A generic output-filtering example helps the raw arm avoid
unnecessary stdout; it does not reduce upstream API transfer. Agent trials use
fresh CLI processes, so separate persistent-cache sensors cannot establish a warm
MCP advantage or a token saving in those trials. Direct CLI execution and launch
through the resolved Node executable use the same tool and corpus controls.
Scoped repository metadata is discovery, not pinned-source proof. Targeted reads
share the output budget without an additional arbitrary line-count cap.

The default surface remains local. `--remote` adds scoped, read-only GitHub research
for the two pinned repositories on both arms (`gh api` GET on raw tools). Remote
preflight and trials share a named profile extending `:read-only` with network access;
the preflight fails before model launch unless GitHub responds and file writes are
denied. Network domain scope remains enforced by the observer, with its existing
post-observation limitation. Local-only trials retain network-disabled `read-only`. Content
and tree requests must select the full locked SHA; indexed code search remains
unpinned discovery. Clone and rewrite operations are outside the model surface.
Use `--questions-file` and `--rubric-file` for a separately frozen campaign, such as
the preserved [eight-trial v5 frame](../../../../.octocode/octocode-eval-benchmark/all-tools-20260913/eval/FRAME.md).

Question-template labels resolve to literal paths before delivery. The observer
accepts the CLI's single query, array, and exact `{queries:[...]}` envelope forms,
including canonical response offset/length/snapshot fields, checking every query
against corpus scope. A malformed `--queries` value is recorded as the CLI's actual
recoverable syntax error, so a repaired call may use the remaining shared budget.
Catalog and context discovery use strict read-only flags. Literal newlines inside a
quoted argument are permitted; unquoted command composition, expansion, and source
writes remain forbidden. Raw tools may read a known in-scope path directly. Schema
discovery and failed calls consume the same budget. Arm order alternates across cases
and passes, including a one-pass campaign.

After all peers have stopped changing the candidate, run the fixed three-pass pilot:

```bash
python3 packages/octocode-benchmark/compare/advanced-research-v1/pilot.py \
  --run --passes 3 --cases A01 A02 \
  --corpus-receipt .octocode/octocode-eval-benchmark/all-tools-20260913/eval/preflight-v6/preflight.json \
  --output-dir .octocode/octocode-eval-benchmark/local-research-next
```

This launches twelve serial, fresh `gpt-5.6-terra` sessions at medium reasoning,
alternating arm order across cases and passes.
Defaults remain 12 shell calls, 240 seconds,
24,000 raw output bytes per call, 100,000 total bytes, and a 900-word answer. Discovery
and failed calls consume those budgets. The twelve deadlines allow 48 minutes plus
preflight/cleanup. This public pilot is diagnostic; held-out source questions and
independent quality review are required for a release claim.

After each sealed trial and at final postflight, the controller checks candidate,
corpus, runner, questions, rubric, catalog, and schemas. Drift stops the next launch
and retains completed receipts plus an immutable partial report. Root HEAD changes,
measured source/runtime edits, and rebuilt or deleted bundled files can invalidate
a run. Unrelated unstaged docs outside the fingerprint are not automatically drift.
The observer cannot guarantee a veto before a violating command executes.

## Review and verification

Verify receipt/answer hashes, then apply every component of each frozen rubric check
to cited source behavior. Keywords alone earn nothing. Keep factual contradictions,
missing coverage, and citation imprecision separate. A function-entry citation proves
only what its cited declaration establishes; body behavior needs its body range. Absent answers are unscorable; intention-only
text has no evidence coverage. Record execution eligibility independently and compare
only valid pairs. Independent held-out review is still required for acceptance.

### Seal and validate each quality review

Grade one packet in one fresh review context. The submitted grade must carry
`schemaVersion: 1`, the packet's canonical `packetSha256`, and a per-label
`answerSha256` map. Compute hashes with the helpers in `grade_validation.py`:

```python
packet_sha = grade_validation.sha256_json(packet)
answer_hashes = {label: grade_validation.sha256_text(text)
                 for label, text in packet["answers"].items()}
```

Each atom's `answerQuotes` is an exact substring from that label's supplied answer.
Every `answer: path:start-end` source anchor must fit inside a source-line
citation printed by that same answer. Path matching is exact or a real relative suffix;
a bare filename is rejected if the answer cites more than one path with that filename.
Reviewer-only comparison anchors can use another prefix, such as `control:`, but never
support the answer's citation score. The validator also rejects label changes,
duplicate/missing atom numbers, invalid enums, and score sums that disagree with atom
records. Line ranges accept hyphens or en dashes, same-prefix abbreviated endpoints
(`1955-69`), and comma continuations, including separate Markdown code spans. An explicit
short reference such as `:1970-1973` inherits the preceding same-line file, or the sole
file referenced earlier in its paragraph. Multiple-file ambiguity and prefix-wrapping
endpoints such as `1998-02` remain rejected. Commit metadata can use
`answer:commit:<full-SHA>` or `answer:<commit-URL>` when that exact identifier is printed
in the answer. The rubric, not the parser, determines whether a commit identifier proves
the claim; it does not prove arbitrary source implementation. Unsupported syntax needs
manual review, not an automatic loss of citation credit.

```bash
python3 packages/octocode-benchmark/compare/advanced-research-v1/grade_validation.py \
  --packet /ABS/judging/local-03/packet.json \
  --grade /ABS/judging/local-03/grade.json
```

This is an integrity check, not a semantic judge: it cannot prove the source range
actually establishes a behavioral claim. Keep independent source-grounded review for
that decision. Older grade artifacts without these binding fields remain historical
records and are intentionally rejected by this validator.

Provider total tokens are input plus output; cached input and reasoning are subsets.
Absent optional cached, reasoning, or cache-write counters remain unknown without
invalidating a present total. Malformed present values still fail the receipt.
Raw command bytes do not prove model-visible context or provider tokens. Keep
structural, lexical, semantic, indexed, and agent-quality measurements separate.

```bash
python3 -m unittest discover -s packages/octocode-benchmark/compare/advanced-research-v1 -p 'test_*.py'
python3 -m unittest discover -s packages/octocode-benchmark/compare/bin -p 'test_*.py'
```

The [controller regressions](test_pilot.py) and
[response-normalization regressions](../bin/test_response_summary.py) are the
executable contracts for repaired measurement failures. Never change the rubric,
budgets, or old receipts to make an excluded trial eligible.
The controller suite executes the supported JSON query envelope through the real CLI
on a corpus path containing spaces and verifies exact returned source evidence.
