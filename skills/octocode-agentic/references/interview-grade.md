Load when grading, reproducing, fixing and closing a [cold interview](interview.md) round.

# Grade and close an interview round

## Grade

- Replay every cold call yourself with [interview-grade.mjs](../scripts/interview-grade.mjs) (`--help` lists options and outcomes): `node <skill-folder>/scripts/interview-grade.mjs <round-dir> --cmd '<runner {tool} {input}>' --reject <regex> --empty <regex>`. The runner, run by `sh`, must accept both substituted arguments. For the local Octocode CLI, from the checkout root: `--cmd 'node packages/octocode/out/octocode.js {tool} {input}' --reject 'errorCode: invalidInput' --empty '# status: empty'`. The grader skips steps whose `input` holds a string value that is wholly a `<…>` placeholder (`"<path>"`, not `"Vec<u8>"`); exclude intended-error scenarios with `--exclude`. Calls run to completion unless you pass `--timeout`. Exit 2 means malformed lines or no gradable call: fix them before reporting. Report `total` and each `groups.<GROUP>` `validRate`, `gradable` and `rejected` as printed; `graded.json` keeps every failing call's full output. Run [its tests](../scripts/tests/interview-grade.test.mjs) with `node --test` after changing the grader.
- Phase B prediction misses are findings: the agent's model of the output was wrong.
- Judge observed behavior, not self-rated confidence.
- Before calling a missing lead or field a regression, compare with schema defaults applied: surfaces that omit default-valued fields make exact-match graders and agent predictions fail on correct output.

## Reproduce and prioritize

- Reproduce every Critical and High finding live before fixing; drop or downgrade what does not reproduce.
- Rank: silent wrong evidence > valid intent rejected > misleading error or recovery > undocumented output shape > wording.
- Use independently reproduced gaps to prioritize work; agreement alone does not establish a defect.
- Separate three causes: contract (fix it), environment (missing provider, auth, gate: report it), model (agent ignored a correct rule: fix only by moving the rule to where that agent looked).
- Interviewees also see their own host's connected servers. A quote that appears nowhere in the pack is environment: check the pack before treating it as a contract conflict.

## Fix

- Fix in the [owning layer](tool-descriptions.md#layer-owners).
- Put the rule where the failing agent looked; a field note beats a server-wide line for a field trap.
- Prefer a correct runtime error or recovery lead over more prose.
- Budget-neutral ([merge or delete a rule first](prompt-writing.md#write-and-repair)); respect host caps on instructions and descriptions.
- Rerun contract tests and the previous round's failing calls after every batch.

## Repeat or stop

- Rebuild the pack from the fixed live surface; new agents; [fresh scenarios](interview.md#pack).
- Stop when a round with fresh scenarios reproduces no new Critical or High finding, or the authorized budget is spent. Use [evaluation](evaluation.md) for an improvement claim.
- When a gap may be a model limit, rerun that group on a second small model family; to measure what the instructions are worth, run one round without them.

## Report

Follow [output.md](../output.md): the executed checks and their outcomes (including gradable calls, valid cold and rejections when measured), `## Changes` per fix with the evidence that drove it, `## Findings` ranked by recurrence and severity, and decisions the owner must make.
