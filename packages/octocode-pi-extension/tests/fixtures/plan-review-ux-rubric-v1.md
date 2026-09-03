# Plan Review UX Rubric V1

Status: Frozen before TUI implementation

## Sample

- Recruit at least five participants who did not implement the feature.
- Each participant completes all ten tasks in `plan-review-ux-v1.json`.
- Randomize task order per participant while preserving prerequisite state inside a task.
- Record the current product baseline against the same corpus before changing the generic question or plan-review TUI.
- Human baseline and target receipts stay local; no telemetry is uploaded automatically.

## Assistance

A task is **without assistance** only when the moderator gives no hint, navigation instruction, terminology explanation, or recovery step after the task begins. Clarifying the written task before the timer begins is permitted and must be recorded.

## Observation record

Each of the 50 observations records:

- corpus/rubric version;
- participant ID and task ID;
- randomized ordinal and terminal profile;
- selected path/action;
- final review phase;
- revision identified by the participant;
- whether Accepted and Executing were distinguished correctly when asked;
- completion and moderator-intervention booleans;
- accidental-Start boolean;
- elapsed time in seconds;
- one short confusion note with no secrets or raw RFC content.

## Scoring

An observation passes only when:

1. the participant reaches the task's declared `success` state;
2. no prohibited event occurs;
3. no moderator hint is given after start; and
4. the participant distinguishes Accepted from Executing correctly whenever the task asks.

Aggregate score is passing observations divided by 50. Default rollout and old-path removal require at least 45/50 passes (90%) and zero accidental Starts. Any accidental Start fails the launch gate regardless of aggregate score.

## Density

Render the deterministic `defaultDensityFixture` at exactly 80 columns with three ordinary options. Count visible terminal lines occupied by the prompt, option rows, focused detail, help, and discussion escape; exclude the submitted-result line. The new default must use at most 60% of the baseline line count.

## Gate ownership

Deterministic tests, security checks, and real integration smokes gate merge/release acceptance. This moderated rubric additionally gates default rollout and removal of the old UX. A failed human gate keeps the new path non-default; it does not convert a failing deterministic check into success.
