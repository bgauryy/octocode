# Answering a benchmark question (a runner)

**You are one of three separate people/agents on this question, working alone.** The other runner (the competing CLI) and the grader are different and you never contact or observe them. You get one question, one assigned CLI, and that CLI's fixed primer from [`RUNNER_TOOL_CONTEXT.md`](RUNNER_TOOL_CONTEXT.md). Read the primer before the first call. Research independently; do not look for the other runner, the grader, or any hidden reference — seeking them invalidates the run.

Write down two things:

- **Answer** — directly answer every material part, with precise evidence anchors (repo, file, symbol, ref/SHA) and honest limitations.
- **Research steps** — briefly, what you checked and how it supports the answer.

Prefer current primary evidence. If evidence is insufficient, say **Unknown** and explain the gap. Don't pad, don't grade yourself, don't name the tool just to advertise it.

There is no required wording, length, citation format, or tool sequence. Also record, for the write-up: each command you ran and its character length (model-out), the measured output characters it pulled in (model-in), your final answer's characters (model-out), the number of calls, and elapsed time. The scored figure is the **total = model-in + model-out** (all chars through the model, both directions). When a matchup supplies an instrumentation wrapper, its JSONL and preserved output artifacts are authoritative; never reconstruct counts from the visible transcript.

Octocode arm: every research command is `npx octocode tools <tool> …` — no MCP, no monorepo entrypoint. A matchup may provide a transparent measurement wrapper whose child process is exactly that command; it may capture output but must not alter the research request or response.

The fixed primer is setup context, not measured CLI output. Any catalog, help,
schema, or failed command you invoke after receiving it is a measured call.

For structured files, exact object/field membership requires an unminified exact read or deterministic parsing. A compact view that elides section boundaries is not sufficient evidence.
