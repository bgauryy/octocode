# Run ledger and hooks

Load for substantial, multi-surface, multi-turn, subagent, saved-brief, or high-confidence work. Quick answers do not need the harness.

## Run Ledger

Start only when local writes are acceptable. Tests can set `OCTOCODE_BRAINSTORM_RUN_DIR`.

```bash
node <skill_dir>/scripts/brainstorm-run.mjs start --idea "<idea>" --mode Validate --surface-plan '{"local":"active","web":"active"}'
node <skill_dir>/scripts/brainstorm-run.mjs checkpoint --run-id <id> --stage research --summary "<delta>" --claim "claim -> source -> confidence" --source "<path-or-url>"
node <skill_dir>/scripts/brainstorm-run.mjs finish --run-id <id> --verdict worth-prototyping --decision "Build RFC" --summary "<result>"
```

Checkpoint when the surface plan, decisive evidence, confidence, or final synthesis changes. Record both sides of material conflicts and the final concession.
Never create one memory entry per checkpoint.

## Hook Entrypoint

`scripts/brainstorm-run.mjs hook` reads the host hook JSON on stdin (`session_id`, `stop_hook_active`). `hooks/hooks.json` wires two Claude-compatible events; other hosts need their native hook surface.

A run belongs to the session that started it: `start --session-id <id>`, else `CLAUDE_CODE_SESSION_ID`.

| Event | Behavior with the newest unfinished run of this session |
|---|---|
| UserPromptSubmit | emits bounded context: run, stage, latest summary |
| Stop | exits 2 once per stop until `finish`; never for another session, an unowned run, or a run idle for 24h; `OCTOCODE_BRAINSTORM_NO_STOP_GATE=1` bypasses |

Hooks stay fast, deterministic, workspace-scoped, and fail-open except the deliberate Stop reminder. They never search, call models, load `.env`, or create folders.

## Self-test

```bash
node <skill_dir>/scripts/brainstorm-run.mjs --self-test
```

It creates the run directory and prints it.

## User Communication

Send progress notes at meaningful boundaries: the Surface Plan, a material research checkpoint, and the final decision are the usual set. Summarize workers; do not paste transcripts.
