# Execute and debug the reasoning loop

Load when `scripts/run-loop.mjs` reports a packet, transport, claim-consistency, or APPLY failure. Normal use stays on the compact runner.

## Preferred path

```sh
node scripts/run-loop.mjs --input compact.json --dry-run
node scripts/run-loop.mjs --input compact.json
```

Use `assets/run-loop-input.schema.json`. The runner routes, derives a minimal DecisionBrief, validates, evaluates, checks claim consistency, and creates provisional APPLY. It exits before the API for inert calls, direct checks, missing facts, scope mismatch, or exhausted crossroads. `--response FILE` replays offline; `--output DIR` selects artifacts.

Every request contains exactly `model`, `state`, and `questions`. `state.reasoning` is a bounded summary with observations and uncertainty—not hidden chain-of-thought. Testable triage also requires predictions, weakening conditions, and branch outcomes.

## Diagnose one layer

```sh
node scripts/route-decision.mjs --input routing-state.json
node scripts/build-decision-packet.mjs --input builder-input.json > request.json
node scripts/validate-decision-packet.mjs --route <route> --input request.json
node scripts/jev.mjs evaluate --input request.json --dry-run
```

Errors identify the failing JSON path and expected/received scope where applicable. Repair input; never weaken scope or precommitment gates.

For direct transport diagnosis:

```sh
node scripts/jev.mjs evaluate --input request.json --retries 0 --timeout-ms 10000
```

For disputed claims, pin the model and retain request binding:

```sh
node scripts/research.mjs --input request.json --retries 0 --timeout-ms 10000 > envelope.json
node scripts/check-research.mjs --request request.json --response envelope.json
```

Status/basis disagreement exits 4 with `suggestion.strategy=report_observed_facts`. Reopen suggested evidence; report only scoped observations or retrieve new evidence. Never repeat-vote unchanged state.

Direct `scripts/apply-response.mjs` debugging requires request, response, actions, and net action. The compact runner removes that map by deriving actions from host-supplied state; provide `actions` plus `netAction` only to override it.

Exits: `0` ready/applied; `2` local input/configuration failure; `3` transport failure; `4` response, claim consistency, or policy block. A selected hypothesis or basis is worth checking, never fact.

Next: wire → `references/protocol.md`; config → `references/configuration.md`; efficacy → `references/benchmark.md`.
