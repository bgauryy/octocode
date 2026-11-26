Load when measuring whether a host loads the right skill.

# Skill activation

## Build the request sets

- A host-activation claim needs host runs; description-only routing checks whether descriptions separate skills, not whether the host loads them.
- Write requests before editing any description: implicit intent, missing details ("why is this page slow?" with no URL), near-misses owned by a neighbor skill, requests that need no skill.
- Tuning set inspected while editing; sealed-test set ([splits](eval-cases.md#splits-and-exposure)) written before tuning results open, run once on final descriptions.
- Run from a small representative project, not an empty folder.

## Run through the host

Write the requests once as `requests.jsonl`, one `{"id","set","expected"}` per line (`expected`: the skill that must load first, or `null` for a no-skill request). Save each run's stream as `runs/<arm>/<id>.jsonl` (`<id>.<repeat>.jsonl` for repeats):

```bash
claude -p "<request>" --model <model> --output-format stream-json --verbose --no-session-persistence > runs/<arm>/<id>.jsonl
```

- Run each request in a fresh headless session with the production tool and permission policy. Keep the same policy, timeout, and concurrency for both arms; bound concurrent runs to avoid resource contention. Record host version, model and policy beside the runs. A restricted-tool check measures that restricted condition only.

## Score

Run [activation-tally.mjs](../scripts/activation-tally.mjs) (`--help` documents inputs, outcomes and exit codes); it reads files and changes nothing. Each run's `firstSkill` is the first top-level `Skill` call's `input.skill`:

```bash
node <skill-folder>/scripts/activation-tally.mjs runs --expected requests.jsonl
```

| Outcome | Counts as |
|---|---|
| `correct` | The expected skill loads first |
| `wrongRoute` | Another skill loads first; `winners` names it |
| `miss` | A required skill does not load: the agent answers, asks a clarifying question, or works with other tools |
| `correctRejection` | No skill loads for a no-skill request |
| `falseTrigger` | A skill loads for a no-skill request |
| `runError` | No skill call and the run did not finish cleanly: rerun it, never count it as a miss |

- Report `sets[]` (correct, misses, wrong routes and false triggers per set and arm, with run counts) and the host version and model from `runs[]`. Exit 1 means `missing[]` requests or `runError` runs: rerun them or report the coverage gap. Exit 2 is bad input, such as a run file that names no request.
- Run each arm at least twice; a difference between arms smaller than the difference between two runs of one arm is not a result.

## When skills under-trigger

- Misses outnumbering wrong routes mean the agent used its own tools; fix the description via `octocode-skills`.
- Retest on the sealed-test set; check no-skill requests for new false triggers.
