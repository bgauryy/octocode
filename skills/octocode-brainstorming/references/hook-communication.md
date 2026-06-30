# Hook Communication And Eval Harness

Load when a brainstorm is substantial, uses subagents, spans turns, or needs a high-confidence final answer. The harness is optional for quick answers.

## Run ledger

Start a ledger when the user asks for best results, RFC follow-up, multiple perspectives, a saved brief, or when the run spans multiple active surfaces. The ledger is the durable claim ledger: every material claim should eventually appear as a checkpoint with source and confidence, especially when evidence conflicts.

```bash
node skills/octocode-brainstorming/scripts/brainstorm-run.mjs start \
  --idea "<user idea>" --mode Validate \
  --surface-plan '{"local":"active if repo-relevant","githubPackages":"active","web":"active"}'
```

Record checkpoints at the three communication points and whenever a material claim changes confidence:

```bash
node skills/octocode-brainstorming/scripts/brainstorm-run.mjs checkpoint \
  --run-id <id> --stage research --summary "what changed" \
  --claim "claim -> source -> confidence" --source "path-or-url"
```

Finish only after the final answer has verdict, decision, and next step:

```bash
node skills/octocode-brainstorming/scripts/brainstorm-run.mjs finish \
  --run-id <id> --verdict worth-prototyping --decision "Build RFC" \
  --summary "one-sentence result"
```

At finish, produce one awareness capture packet from the final surviving verdict, or record a
`doNotCaptureReason` when nothing durable survived. Use the awareness learning-capture reference
for the packet fields and reference grammar; do not write one memory per checkpoint.

Run files live in `.octocode/brainstorming/runs/` by default, so start a ledger only when local writes are acceptable. Override with `OCTOCODE_BRAINSTORM_RUN_DIR` for tests.

When evidence conflicts, record both sides as separate checkpoints and add a final checkpoint naming the concession or unresolved decision point. This prevents the perspective review from becoming theater: the final answer must be able to point back to the ledger entries that survived and the ones that were dropped.

## Hook entrypoint

`scripts/brainstorm-run.mjs hook` is designed for hook systems that pass JSON on stdin. `hooks/hooks.json` contains optional hook wiring for `UserPromptSubmit`, `Stop`, `SubagentStop`, and `SessionEnd`; all events are no-ops unless a run ledger exists.

- `UserPromptSubmit`: if an active run exists, emits bounded `additionalContext` with run id, stage, latest summary, and missing final-answer pieces.
- `Stop`: if an active run exists and no finish was recorded, exits `2` once to remind the agent to finish or explicitly bypass with `OCTOCODE_BRAINSTORM_NO_STOP_GATE=1`.
- `SubagentStop`: records that a subagent finished; the agent must still summarize useful claims into a checkpoint.
- `SessionEnd`: records that a session ended with an active run.

Keep hook scripts deterministic, fast, and fail-open except the deliberate Stop reminder. Do not make hooks search the web, call models, or inspect secrets.

## Eval harness

Evaluate final answers against structured cases:

```bash
node skills/octocode-brainstorming/scripts/eval-brainstorm.mjs --list
node skills/octocode-brainstorming/scripts/eval-brainstorm.mjs \
  --case idea-validation --input /tmp/answer.md --json
```

The evaluator checks observable answer behavior: mode, surface plan, citations, perspective review, decision label, next step, and forbidden failure modes. It does not prove that the market or technical judgment is correct; human review still judges substance.

## Communication protocol

Use only three user-visible progress notes:

1. **Surface Plan**: active/skipped surfaces and why.
2. **Research Checkpoint**: strongest new evidence, weakest claim, and whether a gate fired.
3. **Final Decision**: verdict, decision label, confidence, next proof or RFC handoff.

Do not paste raw subagent transcripts. Summarize what survived review and cite the evidence.
