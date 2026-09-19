# Octocode Jev reasoning loop

Source offloading and bounded judgments with TypeSafe Jev. Pass paths and a question before reading candidate bodies; Octocode gathers source for Jev, and the host verifies selected evidence and owns conclusions. Jev returns typed probabilities, not new facts.

Use native source questions to judge several claims across files, scout to prioritize candidate reads, and evidence-based routes for unresolved choices. Exact facts and execution remain with the host.

## Run

Install with `npx -y octocode skill install octocode-jev-reasoning-loop`. Supply `OCTOCODE_JEV_KEY` in the process environment. If it is stored in `<home>/.octocode/.env`, explicitly load that trusted file with Node’s `--env-file` option as shown in `references/configuration.md`.

Before substantial source reading, use native `jevReasoning route:source_questions` with `sources` (absolute paths and optional line ranges) and `questions` (IDs mapped to independent bounded claims). All selected files share one provider request; no evidence packet is required. Inspect the compact route schema with `octocode tools jevReasoning --scheme --scheme-view query --scheme-select route=source_questions --json --compact`. Use supported/contradicted/insufficient/conflicting judgments to choose a deciding read or test. See `SKILL.md` for an executable example.

When the question is which candidates to read, use native `jevScout` with `source.local`, `claim`, retrieval `anchors`, and `includeEvidence: true`. Inspect returned read/gray_read excerpts and widen incomplete evidence. Use `taxonomy: "relevance"` for question relevance; default `implements` grades capability implementation. Known cheap reads need no Jev. Details: `references/scout.md`.

For standalone use with already-selected evidence, the reasoning runner can derive a packet:

```bash
node <skill-dir>/scripts/run-loop.mjs --claim "Cancellation prevents late cache writes." \
  --scope "Current request completion path" --evidence src/request.ts:20-48 \
  --evidence src/cache.ts:10-25 --model jev-1.13.0
```

Replace the example spans with observed, complete evidence; include counterevidence. Paths resolve from the workspace root. The shortcut derives only evidence IDs, source labels and the default decision goal; `--goal` overrides that goal. `--max-chars` raises each span's bound from 1200 to at most 4000. Oversized spans fail before evaluation. This is the `hallucination_gate` route for unresolved grounding, not a mandatory final check. Exact reads/tests that settle a claim need no Jev call. Artifacts default to `<workspace>/.octocode/octocode-jev-reasoning-loop/`.

For competing hypotheses or plan review, create a compact input using `assets/run-loop-input.schema.json`:

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

Every reasoning-loop decision packet contains a bounded reasoning summary (`observations` and `uncertainty`, with optional assumptions, inferences, counterpoint, and meaningful precommitments). It is an auditable decision summary—not hidden chain-of-thought or private scratch. Testable hypothesis state still requires predictions, weakening conditions, and branch outcomes.

If claim status and selected basis disagree, the runner blocks and emits a deterministic observed-facts narrowing. Verify the narrower claim against original evidence, reusing it when already inspected, complete and current; retrieve changed, incomplete or unseen evidence. Never repeat-vote unchanged state. The current policy permits one judgment per crossroad; another call needs materially changed state or a new decision whose next action could change.

## Scout and source profile

Prefer native `jevScout` to prioritize candidates and return selected excerpts together. `scripts/scout.mjs` is the standalone metadata-only alternative. Use `scripts/profile.mjs` after sources are selected to obtain typed semantic judgments without loading their bytes into the host-model context first. A profile accepts root-relative paths or inline text, sends every independent aspect for one source in a single request, and evaluates separate sources concurrently. See `references/scout.md`, `references/profile.md`, and `assets/profile-input.schema.json`. Both outputs are provisional; ground assertions in original source, reusing already inspected, complete and current evidence.

## Contracts and low-level debugging

Request schemas are under `assets/`; `assets/default-policy.json` owns host thresholds. For packet internals and direct wrappers, read `references/research.md`, then use `route-decision.mjs`, `build-decision-packet.mjs`, `validate-decision-packet.mjs`, `jev.mjs`, `research.mjs`, `check-research.mjs`, or `apply-response.mjs`. Standalone reasoning execution should stay on `run-loop.mjs`; native source questions and scouting do not require a decision packet.

The bundled native binary is `bin/octocode-jev-darwin-arm64` for Apple Silicon macOS. On another platform install Rust 1.85+ and a C linker, then run `npm run build`; `Cargo.toml`, `Cargo.lock`, `src/main.rs`, and `scripts/build.mjs` own the native build. `scripts/octocode-config.mjs` is injected by `@octocodeai/config` for standalone use.

## Verify and benchmark

```bash
npm test
node scripts/eval-recovery-heldout.mjs
node scripts/eval-recovery-heldout.mjs --live --output <workspace>/.octocode/octocode-jev-reasoning-loop/benchmark/recovery-v1
```

- `evals/decision-cases.json` freezes deterministic route cases.
- `evals/recovery-heldout.json` freezes semantic recovery/control cases.
- `evals/kpi-contract.json` owns KPI and acceptance boundaries.

The primary semantic KPI is **Wrong-Lean Recovery Rate**. Live results also report false recovery, unsupported claims, calls, and model tokens. Without a matched host-only baseline, they characterize behavior but do not prove comparative efficacy. See `references/benchmark.md`.
