# Answering a benchmark question (a runner)

**You are one isolated runner agent in a pairwise matchup, working alone.** The matchup pits
**Octocode (anchor)** against **one baseline CLI**; you are exactly one of those two. The
other runner and the judge are separate — you never contact or observe them. You get one
question, one assigned CLI, and that CLI's fixed primer from
[`RUNNER_TOOL_CONTEXT.md`](RUNNER_TOOL_CONTEXT.md). Read the primer before the first call.
Seeking the other runner, the judge, or any hidden reference invalidates the run.

Your assigned arm is exactly one of (arm id in **bold** — use it in your output path):

- **`octocode`** — every research command is `npx octocode@<ver> tools <tool> --queries …` (via `bin/octoc`); no MCP, no gh.
- **`rtk`** — every research command is `rtk gh <args>` (read-only, via `bin/rtkm`).
- **`headroom`** — every research command is `./bin/ghc <gh args>` (compressed).
- **`gh`** — every research command is `gh <args>` (read-only, via `../bin/ghm`).

## What to produce (one `## Q<n>` answer section — required)

Append your answer for this question as a `## Q<n>` section to your arm/pass answers file
`answers/<arm>-p<pass>.md` (e.g. `answers/octocode-p1.md`, `answers/rtk-p1.md`) — the exact
path the blind-packet builder and campaign validator read. The heading MUST be `## Q<n>`
(so headings across the file are exactly `Q1..QN` in order). This section — not the
transcript, not the wrapper log — is what the judge reads; if it is missing your arm scores
nothing on that question. Each section has two parts:

- **Answer** — directly answer every material part, with precise anchors (repo, file,
  symbol, ref/SHA) and honest limitations (say **Unknown** if evidence is insufficient).
- **Research steps** — briefly, what you checked and how it supports the answer.

**Stats are recorded by the instrumentation, not hand-written.** Every research command runs
through your arm's wrapper (which appends `model_in_chars` / `model_out_chars` per call to
the run JSONL), and your final answer is logged with `bin/record_answer.py` (pure
model-out). The scored figure is **total = model-in + model-out** from that log — the JSONL
and preserved artifacts are authoritative; never reconstruct counts from the transcript.

## Leanest legal path (required)

Take the smallest sufficient path your tool allows: targeted region reads (`matchString` /
line ranges), snippet-bearing searches, minimal `--json` fields, raw file media. **Do not
pull a whole git tree or a whole large file when a targeted read or search answers the
question** — that inflates your char cost and is a fairness violation. `gh` has no
server-side region read, so a whole-file `raw` fetch is legitimate only when you truly need
most of the file. Freeze every mutable ref (branch/PR-state/SHA + UTC) before answering and
use the frozen ref.

Prefer current primary evidence. Don't pad, don't grade yourself, don't name the tool to
advertise it. There is no required wording, length, citation format, or tool order.

The fixed primer is setup context, not measured output. Any catalog, help, schema, or
failed command you invoke afterward is a measured research call.

For structured files, exact object/field membership requires an unminified exact read or
deterministic parsing — a compact view that elides section boundaries is not proof.
