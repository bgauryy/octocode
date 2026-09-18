# Octocode Jev Reasoning Loop

Use TypeSafe Jev as a bounded judgment inside an evidence-driven host reasoning loop. Its value is decision discipline, not solution generation: the host creates alternatives, retrieves facts, runs checks, and owns conclusions; Jev challenges or selects among supplied options only where semantic uncertainty can change the next action.

## Core loop

```text
HOST REASONING
  → BOUNDED ALTERNATIVES
  → JEV TYPED JUDGMENT
  → PRECOMMIT PREDICTION
  → DISCRIMINATING CHECK
  → REAL EVIDENCE
  → JEV DELTA JUDGMENT
  → UPDATE / ABANDON / REFRAME
  → CLAIM CHECK
  → EVIDENCE GATE
```

The precommitment is the key discipline. Before checking reality, record what each testable hypothesis predicts and what observation would weaken it. This makes contradictory evidence harder to rationalize away after the fact. Do not force predictions or falsifiers onto classificatory and claim-status questions where those fields add no meaning.

A Jev call is warranted only when its typed answer can alter the next action. Deterministic lookups, arithmetic, authorization, stale or missing facts, empty grounding, and already-obvious checks remain host/code work.

## Capabilities

- Routes hunches, competing testable explanations, expensive decisions, evidence deltas, disputed claims, and assertion grounding through separate minimal contracts.
- Keeps all Jev outputs provisional and distinct from real evidence.
- Freezes branchable expected outcomes before empirical checks and compares them with newly observed evidence.
- Supports explicit abandonment or reframing when evidence breaks the current deck.
- Binds claim-check responses to exact requests and rejects inconsistent evidence-basis answers.
- Applies deterministic policy for call limits, cost preferences, grounding thresholds, and scope checks.

## Install and configure

```bash
npx -y octocode skill install octocode-jev-reasoning-loop
```

Put `OCTOCODE_JEV_KEY=...` in `<HOME>/.octocode/.env`; see `references/configuration.md` for precedence and trusted project-env behavior. The bundled `bin/octocode-jev-darwin-arm64` targets Apple Silicon macOS. On another platform install Rust 1.85+ and a C linker, then run `npm run build` inside this folder. `package.json`, `Cargo.toml`, `Cargo.lock`, `src/main.rs`, and `scripts/build.mjs` own packaging and the native build.

## Execute one uncertainty point

```bash
node scripts/route-decision.mjs --input routing-state.json
node scripts/build-decision-packet.mjs --input builder-input.json > request.json
node scripts/validate-decision-packet.mjs --route hypothesis_triage --input request.json
node scripts/jev.mjs evaluate --input request.json --dry-run
node scripts/jev.mjs evaluate --input request.json > response.json
node scripts/apply-response.mjs --request request.json --response response.json \
  --actions actions.json --net-action "Run C1 and record both expected branches"
```

After the selected check returns real evidence, build a `reflection_delta` packet. Use `scripts/research.mjs` plus `scripts/check-research.mjs` for `disputed_inference`. The public wrappers import `scripts/decision-contract.mjs`; `scripts/cli-json.mjs` supplies bounded JSON I/O. `scripts/octocode-config.mjs` is the standalone injected Octocode configuration artifact used by the launcher.

## Contracts

- Private minimal build input: `assets/decision-brief.schema.json`
- Requests: `assets/hunch.schema.json`, `assets/hypothesis-triage.schema.json`, `assets/decision-review.schema.json`, `assets/reflection-delta.schema.json`, `assets/claim-check.schema.json`, `assets/hallucination-gate.schema.json`
- Application: `assets/apply-output.schema.json`
- Host policy: `assets/default-policy.json`

`PLAN.md` maps the implemented design to shipped files and records the remaining empirical boundary.

## Benchmark and verification

The primary semantic KPI is **Wrong-Lean Recovery Rate**: among cases where the initial attractive hypothesis is wrong, how often does contradictory evidence make the host abandon or reframe it? Track tokens, tool calls, unsupported claims, and unnecessary Jev calls as guardrails.

From this skill folder:

```bash
npm test
node scripts/verify-reasoning-loop.mjs
node scripts/eval-decision-loop.mjs
```

From the Octocode repository root, use `npm --prefix skills/octocode-jev-reasoning-loop test`.

`npm test` runs the offline transport, research, decision-contract, schema, native dry-run, and deterministic routing checks without a paid API call. Those checks prove contract behavior, not semantic benefit; controlled held-out variants in `references/benchmark.md` own efficacy claims.

Finally run the `octocode-skills` reviewer against this standalone folder.
