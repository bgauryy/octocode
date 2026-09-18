# Octocode Jev Reasoning Loop

A host-owned decision loop around TypeSafe Jev. The host supplies alternatives, gathers facts, executes checks, and owns conclusions. Jev only returns typed judgments over bounded choices.

```text
HOST REASONING → BOUNDED ALTERNATIVES → JEV JUDGMENT → PRECOMMIT
→ REAL CHECK → REAL EVIDENCE → JEV DELTA → UPDATE / ABANDON / REFRAME
→ CLAIM CHECK → EVIDENCE GATE
```

## Run

Install with `npx -y octocode skill install octocode-jev-reasoning-loop`. Put `OCTOCODE_JEV_KEY=...` in `<HOME>/.octocode/.env`; `references/configuration.md` documents precedence and trusted project environments.

Create a compact input using `assets/run-loop-input.schema.json`:

```json
{
  "route": "hypothesis_triage",
  "model": "jev-latest",
  "willChangeAction": true,
  "state": {
    "mainGoal": "Find the regression cause.",
    "goal": "Choose a discriminating check.",
    "scope": "revision abc",
    "evidence": [{"id":"E1","source":"test.log:1","scope":"revision abc","content":"Warm fails; cold passes."}],
    "hypotheses": [
      {"id":"H1","statement":"Cache is stale.","assumption":"Warm reads cache.","predicts":["Bypass passes."],"weakenedBy":"Bypass fails."},
      {"id":"H2","statement":"Caller branch differs.","assumption":"Warm chooses another branch.","predicts":["Traces differ."],"weakenedBy":"Traces match."}
    ],
    "next_checks": [
      {"id":"C1","action":"Run cache bypass.","cost":"low","expectedOutcomes":[
        {"observation":"Pass.","effect":{"H1":"strengthen","H2":"weaken"}},
        {"observation":"Fail.","effect":{"H1":"weaken","H2":"strengthen"}}
      ]},
      {"id":"C2","action":"Compare caller traces.","cost":"medium","expectedOutcomes":[
        {"observation":"Differ.","effect":{"H1":"weaken","H2":"strengthen"}},
        {"observation":"Match.","effect":{"H1":"strengthen","H2":"weaken"}}
      ]}
    ],
    "unknowns": []
  }
}
```

Then use one command:

```bash
node scripts/run-loop.mjs --input compact.json --dry-run
node scripts/run-loop.mjs --input compact.json
```

The runner routes before contacting Jev, derives minimal DecisionBrief boilerplate, validates the public packet, evaluates, performs bound claim consistency checks, generates deterministic provisional APPLY actions, and saves request/response/apply artifacts. `--response FILE` replays recorded output with no API call. `--output DIR` chooses the artifact directory.

Every public request contains a bounded reasoning summary (`observations` and `uncertainty`, with optional assumptions, inferences, counterpoint, and meaningful precommitments). It is an auditable decision summary—not hidden chain-of-thought or private scratch. Testable hypothesis state still requires predictions, weakening conditions, and branch outcomes.

If claim status and selected basis disagree, the runner blocks and emits a deterministic observed-facts narrowing. Reopen its evidence before using the narrower claim; never repeat-vote unchanged state.

## Contracts and low-level debugging

Request schemas are under `assets/`; `assets/default-policy.json` owns host thresholds. For packet internals and direct wrappers, read `references/research.md`, then use `route-decision.mjs`, `build-decision-packet.mjs`, `validate-decision-packet.mjs`, `jev.mjs`, `research.mjs`, `check-research.mjs`, or `apply-response.mjs`. Normal execution should stay on `run-loop.mjs`.

The bundled native binary targets Apple Silicon macOS. On another platform install Rust 1.85+ and a C linker, then run `npm run build`; `Cargo.toml`, `Cargo.lock`, `src/main.rs`, and `scripts/build.mjs` own the native build. `scripts/octocode-config.mjs` is injected by `@octocodeai/config` for standalone use.

## Verify and benchmark

```bash
npm test
node scripts/eval-run-loop.mjs
node scripts/eval-recovery-heldout.mjs
node scripts/eval-recovery-heldout.mjs --live --output <workspace>/.octocode/octocode-jev-reasoning-loop/benchmark/recovery-v1
```

- `evals/decision-cases.json` freezes deterministic route cases.
- `evals/run-loop-cases.json` compares compact authoring and one-command execution with the legacy workflow.
- `evals/recovery-heldout.json` freezes semantic recovery/control cases.
- `evals/kpi-contract.json` owns KPI and acceptance boundaries.

The primary semantic KPI is **Wrong-Lean Recovery Rate**. Live results also report false recovery, unsupported claims, calls, and model tokens. Without a matched host-only baseline, they characterize behavior but do not prove comparative efficacy. See `references/benchmark.md`.
