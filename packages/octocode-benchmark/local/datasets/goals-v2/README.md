# Local code research: 40 questions

This version contains 40 source-research tasks on nine pinned, complete checkouts. Eight tasks (Q33–Q40) form a Terraform slice. The previous 30-question catalog stays unchanged. These are newly authored development questions on public source, not a sealed generalization test. No model results were used to select or edit them.

`catalog.json` identifies each question and its repositories. `repositories.json` pins full commit and Git tree IDs, the source checkout path, and a digest of the actual tracked-file corpus. A commit ID alone is not sufficient: the runner must verify the working source and exclude untracked material. No question depends on an expiring Octocode cache directory. Terraform was obtained with the built `ghCloneRepo`, then copied by a local Git clone into the stable corpus.

## Runner interface

Read one Qn.md, and replace only its `{{REPO:id}}` token with the isolated checkout path for that catalog row. Reject unknown IDs, leftover tokens, or substitutions outside the trial's permitted roots. Hash the final rendered solver packet. The solver receives the question and the same frozen production-equivalent instructions in both arms. It must not receive this README, the whole catalog, private strata, rubrics, source excerpts, previous answers, or outcomes from another arm.

The public metadata has no reference answers. Private Qn.md rubrics and corpus file manifests are kept in the evaluator's separate storage; the run controller supplies that location only to the judge. A directory name is not isolation: probe access denial with the actual solver tool identity. Limit local tools to the trial's roots; suppress unrelated home/workspace instructions, shared memory, external retrieval, and sibling trial artifacts. Record any isolation limitation instead of claiming a clean held-out run.

## Scope

| Questions | Repository | Local directory |
|---|---|---|
| Q1–Q4 | Lodash | javascript |
| Q5–Q8 | Tokio | rust |
| Q9–Q12 | Django | python |
| Q13–Q16 | Prometheus | go |
| Q17–Q20 | Guava | java |
| Q21–Q24 | Excalidraw | tsx |
| Q25–Q28 | Redis | c |
| Q29–Q32 | Newtonsoft.Json | csharp |
| Q33–Q40 | Terraform | terraform |

There are 13 exact controls, 16 broader investigations, seven call/binding tasks, three structural tasks, and one explicit uncertainty task. A category is an evaluator slice, not a solver hint. A broad task can still be answered most cheaply by literal search. Exact controls, no-benefit outcomes, Unknown answers, unsupported operations, and worse outcomes all remain in the report.

Every question is read-only. Relevant tool opportunities include localSearch, localFetch, structureSearch, astSearch, and lspSearch; correctness never requires a particular tool sequence when another route supplies sufficient evidence. Reordering a full response does not count as an avoided read. A syntax match is not a semantic identity proof, and an import graph is not safe-deletion proof.

## Tool applicability and mutation safety

The separate deterministic applicability lane covers the built local tools on pinned source.

Authoring preflight found a working TypeScript language server resolving the Q21 call. Go LSP returned `serverUnavailable` because gopls was absent; no Go LSP capability is claimed. HCL `.tf` AST returned `languageUnsupported`; exact reads remain available. C# AST reported recovered parse errors and partial coverage; a partial outline is not an exhaustive inventory. The runner freezes server/grammar availability equally for both arms and reports fallbacks explicitly.

The completed applicability lane passed 11 checks with no paid calls. Actual MCP `localFetch` denied both a private rubric path and a sibling question path while permitting the selected corpus. This probes those two paths and tool; the runner must enforce and verify its full per-trial isolation policy. A separate synthetic TypeScript definition probe also passed through MCP from a temporary directory outside the repository, with project trust, auto-installation and prewarming disabled. It used the installed Octocode TypeScript backend without a custom LSP configuration or managed cache. These checks establish capability and boundaries, not question accuracy or model effectiveness.

## Grading and measurement

Each private rubric defines atomic findings, cited source excerpts and file hashes. Content is worth eight points and supported source anchors two. Accept semantically equivalent answers and verified alternative evidence; do not grade by keyword overlap or require an oracle navigation path. Honest Unknown avoids fabricated certainty but earns no missing content points. Unsupported material claims, false completeness, or deletion claims receive a capped score; unauthorized mutation invalidates the read-only trial and its cost remains recorded.

Freeze question/rubric/source hashes before trials. Blind candidate identity for grading, calibrate the judge before comparing arms, and keep grader feedback out of scored attempts. Retain crashes, unavailable tools, retries, cap exits, and invalid trials in separate status/cost fields. Report paired quality before cost, Terraform separately, and uncertainty for small slices.

Count complete tool requests/responses and final answers from real transcripts. Report exact source-verification reads, loaded tool schema context, provider input/output usage, total elapsed time, and unknown usage separately. Never infer total economic savings by subtracting a selected source prefix or by counting only semantic packets. Do not pool the separate mutation/applicability lane into question accuracy.
