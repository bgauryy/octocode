# Write-up template

Save the new campaign at the [output location](../output.md#saved-result). Lead with the correctness verdict and measured character cost. Record the question-set revision and count, the two arm versions, runner and judge models, and the revision of the fixed primers.

## Per-question table (one matchup = Octocode vs its baseline)

One row per question (write one such table per matchup):

| Q | Corr O/B | Depth O/B | Workflow O/B | Chars O / B | Winner | Note |
|---|---|---|---|---|---|---|

O = Octocode (anchor), B = this matchup's baseline (another Octocode build, gh+RTK, gh+Headroom, or plain gh).
`Chars` = model-in + model-out = total_chars; ratios below use total_chars.

## Summary (paired, per question)

Aggregate with [paired statistics](aggregation-and-stats.md). Give one column per matchup actually run. Include correctness, depth and workflow as paired win/tie/loss counts with sign tests. For characters give the geometric mean ratio with its bootstrap interval, median and range, and leaner win count. If including a pooled sum, show the top question’s contribution and the leave-one-out result.

## Best-on (orchestrator roll-up)

State plainly who is best on **accuracy**, **quality/depth**, **workflow**, and
**characters** — overall and where each leads. Name the questions that drive any large
ratio, and disclose any fairness caveat (a non-lean path on any arm).

## Per question (detail)

For each pairing give both short answers, their research steps and their scores
(correctness, depth, workflow, chars) **with the judge's one-line reasoning per score**.
Note fairness caveats explicitly, and flag any question confirmed by order-swap/second judge.
List **unresolved** questions (judges disagreed on correctness) separately — they are
excluded from the tally, and the count of exclusions is stated.

## Bottom line

Whether the arms were equally correct, and — at parity — who was leaner and **where**
(which question shapes). Avoid a single headline multiplier that the per-question data does
not support.
