Load when filling the Findings, Results or Pending section of a [saved report](../output.md#saved-report).

# Saved report sections

## Findings

Use one table, most severe first:

| Location | Finding | Layer | Severity | Consequence | Confidence | Fix |
|---|---|---|---|---|---|---|

Severity is `Critical`, `High`, `Medium` or `Low`. Confidence is `high`, `medium` or `low`. A candidate gets `low`, and its Finding starts with `Candidate:`. A design comparison without defects puts one row per option. For a prompt review, the Finding quotes the affected text and the Consequence states its behavioral effect. Under `## Checks`, say what ran, its result, and what stayed unassessed.

## Results

Give one metric table per split, with development, validation and sealed-test results kept apart. Link the runner's result files ([run folders](eval-harness.md#suite-and-run-folders)) instead of pasting them. Keep baseline and candidate comparable. Charts label units, sample size and uncertainty.

## Pending

Add one row per open item, in plain words:

| Item | Conflict or blocker | Who acts |
|---|---|---|

Include a message or lease identifier only when another agent must act on it; otherwise it stays in the coordination database.
