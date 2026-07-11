# Run Ledger, Hooks, And Evaluation

Load for substantial, multi-surface, multi-turn, subagent, saved-brief, or high-confidence work. Quick answers do not need the harness.

## Run Ledger

Start only when local writes are acceptable. Default storage is `.octocode/brainstorming/runs/`; tests may set `OCTOCODE_BRAINSTORM_RUN_DIR`.

```bash
node skills/octocode-brainstorming/scripts/brainstorm-run.mjs start --idea "<idea>" --mode Validate --surface-plan '{"local":"active","web":"active"}'
node skills/octocode-brainstorming/scripts/brainstorm-run.mjs checkpoint --run-id <id> --stage research --summary "<delta>" --claim "claim -> source -> confidence" --source "<path-or-url>"
node skills/octocode-brainstorming/scripts/brainstorm-run.mjs finish --run-id <id> --verdict worth-prototyping --decision "Build RFC" --summary "<result>"
```

Checkpoint at surface-plan, decisive evidence, confidence changes, and final synthesis. Record both sides of conflicts and the final concession.
Capture at most one durable awareness lesson from the surviving verdict. Never create one memory per checkpoint.

## Hook Entrypoint

`scripts/brainstorm-run.mjs hook` reads JSON on stdin. `hooks/hooks.json` wires these Claude-compatible events; Pi uses its extension adapter and other hosts need their native hook surface.

| Event | Behavior with a matching active workspace run |
|---|---|
| UserPromptSubmit | emits bounded context: run, stage, latest summary, missing pieces |
| Stop | exits 2 until finish; `OCTOCODE_BRAINSTORM_NO_STOP_GATE=1` bypasses |
| SubagentStop | records completion; the main agent still checkpoints useful claims |
| SessionEnd | records an unfinished session |

Hooks stay fast, deterministic, workspace-scoped, and fail-open except the deliberate Stop reminder. They never search, call models, or inspect secrets.

## Eval Harness

```bash
node skills/octocode-brainstorming/scripts/eval-brainstorm.mjs --list
node skills/octocode-brainstorming/scripts/eval-brainstorm.mjs --case idea-validation --input /tmp/answer.md --json
node skills/octocode-brainstorming/scripts/brainstorm-run.mjs --self-test
```

The evaluator checks observable structure and failure modes, not whether market or technical judgment is true.
Cited `file:line` references are always checked locally (no flag, no network — fabricated paths or stale line numbers fail `cited file:line references resolve`).
Cited URLs are only checked live with `--verify-links` (network required, opt-in so default/CI runs stay offline) and only a definitive 404 fails `cited links are reachable (verified)` — timeouts, 403s, 5xx, and hosts that reject `HEAD` are `unverified`, not `dead`.

**Contract: the deterministic score is the gate; `--agentic` is monitoring, not a gate.** `required`/`forbidden`/`binaryQuestions`/citation-count/Sources-section checks decide pass/fail (`affectsScore: false` on the agentic layer is deliberate — a flaky LLM judge must never fail CI).
Advisory-only protects the gate from judge inconsistency, but it does **not** protect against self-preference bias.
If the same model family both writes the brainstorming answer and grades it via `--agentic`, use a **different judge model/provider** whenever that advisory pass is actually informing a decision — not for a curiosity-only check.

## User Communication

Use only three progress notes: **Surface Plan**, **Research Checkpoint** (strongest evidence, weakest claim, gate), and **Final Decision** (verdict, confidence, next proof/RFC handoff). Summarize subagents; never paste transcripts.
