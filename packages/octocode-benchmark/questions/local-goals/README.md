# Local code research: 40 questions

This version contains 40 source-research tasks on nine pinned, complete checkouts. Eight tasks (Q33–Q40) form a Terraform slice. The previous 30-question catalog stays unchanged. These are newly authored development questions on public source, not a sealed generalization test. No model results were used to select or edit them.

Each `Qn.md` names its pinned checkout by path (`packages/octocode-benchmark/local/repos/<dir>`); the repositories and commits are in [local/README.md](../../local/README.md). Before a run, check each clone with `git -C <dir> rev-parse HEAD` and `git -C <dir> status --short`: a different commit or a dirty tracked tree makes answers not comparable with stored ones. No question depends on an expiring Octocode cache directory.

## Running it

Hand a runner one `Qn.md` with the shared runner and arm briefs, and nothing else: not this README, other questions, rubrics, source excerpts, previous answers or another arm's outcome. Private rubrics stay in the evaluator's storage and go to the judge only. Limit the runner's tools to the checkout paths and probe that the limit holds with the runner's real tool identity (try a rubric path and a sibling question path). Suppress home or workspace instructions, shared memory, external retrieval and sibling trial artifacts; record any isolation limit instead of claiming a clean held-out run. Environment setup: [TESTING_ENV.md](../../TESTING_ENV.md).

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

## Language support to expect

Language servers and grammars differ per clone. Check what the runner's machine has (a missing server shows as incomplete coverage, not a pass) and give both arms the same environment. Notes from authoring: TypeScript LSP resolves Excalidraw calls; Go LSP returns `serverUnavailable` without gopls; HCL `.tf` has no AST (`languageUnsupported`, exact reads still work); C# AST can report recovered parse errors, so a partial outline is not an inventory. Report fallbacks explicitly.

## Grading and measurement

Each private rubric defines atomic findings, cited source excerpts. Content is worth eight points and supported source anchors two. Accept semantically equivalent answers and verified alternative evidence; do not grade by keyword overlap or require an oracle navigation path. Honest Unknown avoids fabricated certainty but earns no missing content points. Unsupported material claims, false completeness, or deletion claims receive a capped score; unauthorized mutation invalidates the read-only trial and its cost remains recorded.

Freeze the question, rubric and source commit before trials. Blind candidate identity for grading, calibrate the judge before comparing arms, and keep grader feedback out of scored attempts. Retain crashes, unavailable tools, retries, cap exits, and invalid trials in separate status/cost fields. Report paired quality before cost, Terraform separately, and uncertainty for small slices.

Count complete tool requests/responses and final answers from real transcripts. Report exact source-verification reads, loaded tool schema context, provider input/output usage, total elapsed time, and unknown usage separately. Never infer total economic savings by subtracting a selected source prefix or by counting only semantic packets. Do not pool the separate mutation/applicability lane into question accuracy.
