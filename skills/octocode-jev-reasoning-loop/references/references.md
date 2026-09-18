# Inspected sources

Load when auditing protocol provenance, model limits, or benchmark claims. Why: distinguish observed contracts from local policy and unmeasured performance.

Research snapshot: September 18, 2026. Official pages inspected through the live documentation index and pinned source links:

| Source | Used for |
|---|---|
| [TypeSafe agent skill](https://github.com/typesafe-ai/skills/blob/main/skills/typesafe-ai/SKILL.md) | Live-doc routing, behavior-first design, composition, and failure diagnosis |
| [Introduction](https://docs.typesafe.ai/introduction.md) | Typed independent judgments |
| [How to build](https://docs.typesafe.ai/concepts/how-to-build-with-system-one.md) | Code-owned workflow and narrow decisions |
| [State](https://docs.typesafe.ai/concepts/state.md) | Caller-supplied text/JSON context |
| [Choice](https://docs.typesafe.ai/primitives/choice.md) | Closed decks, probabilities, confidence, and 255-option bound |
| [Score](https://docs.typesafe.ai/primitives/score.md) and [Noul](https://docs.typesafe.ai/primitives/noul.md) | Ordered expectation and yes-probability semantics |
| [Confidence](https://docs.typesafe.ai/confidence.md) | Distribution concentration versus probability |
| [HTTP API](https://docs.typesafe.ai/api.md) | Endpoint, auth, request/response envelope, and failures |
| [JavaScript SDK types](https://github.com/typesafe-ai/typesafe-sdk-js/blob/main/src/types.ts) | Structured entries, nulls, and optional instructions |
| [Models](https://docs.typesafe.ai/models.md) | Alias movement and version pinning |
| [Jev 1.13 caveats](https://docs.typesafe.ai/model-jaggedness/jev-1.13.md) | Context limits, arithmetic/date weakness, and adversarial state |

The SDK entry types are broader than some HTTP examples. The client follows the SDK shapes and validates responses. Model-specific guidance states 32k tokens for state plus the longest question and 64k across state plus all questions; the local 24k working target is headroom policy, not a provider limit. Aliases observed as `jev-1.13.0` may move; record the resolved model.

The September 18 live index reconfirmed Choice, Score, Noul, independent questions, probabilities, and code-owned composition. A targeted building-guide fetch returned usable Choice guidance, while five concurrent CDP fetches failed with zero status; no API field was inferred from those failures.

## Prior measurements and implementation sources
A fixed-budget 80-call claim-check comparison produced 24/24 correct statuses for short instructions versus 23/24 for longer ones. A later six-case integrated trial found no final-answer gain over a 6/6 host baseline. Split pilots also showed correct typed choices but unstable host application; this motivated executable APPLY. These small studies do not prove product benefit.

The browser composition pattern was inspected at [`browser-use/jev-ultrafast@452c1ad`](https://github.com/browser-use/jev-ultrafast/tree/452c1ad2dd628008f1d5608f28158d76e49e6cc0), especially `model.py`, `agent.py`, `snapshot.js`, and `test_agent.py` for fan-out, freshness, control indexing, and selected-target execution.

The Rust client uses `ureq 3.4.2` with Rustls, `serde_json`, and `httpdate`. The vendored `scripts/octocode-config.mjs` is an injected standalone build artifact from `@octocodeai/config`; skill code imports it relatively.

## Current contract family
Private input: `decision-brief.schema.json`. Public routes: hunch, hypothesis-triage, decision-review, reflection-delta, claim-check, and hallucination-gate. Application and policy: `apply-output.schema.json` and `default-policy.json`. `scripts/verify-reasoning-loop.mjs` validates their invariants and native dry-runs all six public routes without API calls.

Next: when implementing packets return to `references/protocol.md`; when measuring benefit use `references/benchmark.md`.
