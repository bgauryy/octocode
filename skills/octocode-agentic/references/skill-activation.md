Load when measuring whether a host loads the right skill.

## Build the request sets

- A host-activation claim needs host runs; description-only routing checks whether descriptions separate skills, not whether the host loads them.
- Write requests before editing any description: implicit intent, missing details ("why is this page slow?" with no URL), near-misses owned by a neighbor skill, requests that need no skill.
- Tuning set inspected while editing; held-out set written before tuning results open, run once on final descriptions.
- Run from a small representative project, not an empty folder.

## Run through the host

```bash
claude -p "<request>" --model <model> --output-format stream-json --verbose --no-session-persistence
```

- Run each request in a fresh headless session with the production tool and permission policy. Keep the same policy, timeout, and concurrency for both arms; bound concurrent runs to avoid resource contention. Record the first `Skill` tool call (`input.skill`) or none, plus host version, model, policy, and cost. A restricted-tool probe measures that restricted condition only.

## Score

| Outcome | Counts as |
|---|---|
| The expected skill loads first | correct |
| Another skill loads | wrong route; name the winner |
| A required skill does not load: the agent answers, asks a clarifying question, or works with other tools | miss |
| No skill loads for a no-skill request | correct rejection |
| A skill loads for a no-skill request | false trigger |

- Report correct, misses, wrong routes, and false triggers per set, with run counts.
- Repeat runs before claiming a few-point difference.

## When skills under-trigger

- Misses outnumbering wrong routes mean the agent used its own tools; fix the description via `octocode-skills`.
- Retest on the held-out set; check no-skill requests for new false triggers.
