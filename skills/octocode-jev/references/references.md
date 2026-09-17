# Inspected sources

Research date: September 18, 2026. Two Terra workers independently inspected the official protocol and browser example; the parent rechecked the HTTP reference, SDK types, model caveats, configuration loader and example transport. This skill documents observed contracts, not untested performance claims.

## Official protocol and behavior

| Source | Used for |
| --- | --- |
| [TypeSafe Agent Skill](https://github.com/typesafe-ai/skills/blob/main/skills/typesafe-ai/SKILL.md) | Live-documentation route, behavior-first design, composition patterns, uncertainty policy and layered failure diagnosis |
| [Introduction](https://docs.typesafe.ai/introduction) | Typed primitives and independent atomic judgments |
| [HTTP API](https://docs.typesafe.ai/api) | Endpoint, auth, request/response envelopes, HTTP failures |
| [Current SDK types](https://github.com/typesafe-ai/typesafe-sdk-js/blob/main/src/types.ts) | Structured descriptions, null entries and optional instructions absent from the narrower HTTP examples |
| [State](https://docs.typesafe.ai/concepts/state) | Text/JSON modality and caller-supplied context |
| [Choice](https://docs.typesafe.ai/primitives/choice) | Closed-set choices and 255-option bound |
| [Confidence](https://docs.typesafe.ai/confidence) | Confidence vs probability; domain-specific policy |
| [Models](https://docs.typesafe.ai/models) | Model aliases, version pinning and models endpoint |
| [Jev 1.13 caveats](https://docs.typesafe.ai/model-jaggedness/jev-1.13) | September 16 context limits, arithmetic/date weaknesses, adversarial state, no generation |
| [System One/Jev launch](https://typesafe.ai/blog/introducing-system-one-models-and-jev) | Product rationale; structured decisions composed in code |

The protocol reference records two observed documentation differences: SDK entry types are broader than HTTP narrative examples, and the newer model-specific context guidance is more precise than the general primitive page. A live smoke test on September 18, 2026 loaded the user's key from the global Octocode `.env` and validated Choice, Score and Noul responses from `jev-1.13.0`. Automated regression tests remain offline; a smoke test does not measure judgment quality across a domain.

The model-specific page's [Markdown source](https://docs.typesafe.ai/model-jaggedness/jev-1.13.md), rechecked September 18, specifies 32k for state plus the longest question and 64k across state plus all questions; it does not verify a 34k limit. The context guide's approximately 24k estimated-token working target is a local headroom policy, not a provider claim. The same source warns against irrelevant state, indirection and treating adversarial content as safe.

## Research pilot

The research contract retains the short prompt from a fixed-budget 80-call comparison on jev-1.13.0. Its twelve held-out cases, repeated twice, produced 24/24 correct claim statuses versus 23/24 for longer instructions. The longer version once paired supported with none on a partial-search case. The local guard rejects that inconsistency; the v2 command also binds the response to the exact request and allows caller-declared multi-source bases. Neither mechanism certifies source truth. A later six-case integrated trial found no final-accuracy gain over the 6/6 host baseline, while all twelve Jev calls were coherent and correct; the optional routing decision follows that measured zero delta. Evaluation artifacts remain workspace-owned under `<workspace>/.octocode/octocode-eval-benchmark/`, not a runtime dependency of this standalone skill.

## Reference implementation

Repository: [browser-use/jev-ultrafast](https://github.com/browser-use/jev-ultrafast), MIT licensed, inspected at commit `452c1ad2dd628008f1d5608f28158d76e49e6cc0`.

| File | Used for |
| --- | --- |
| [model.py](https://github.com/browser-use/jev-ultrafast/blob/452c1ad2dd628008f1d5608f28158d76e49e6cc0/jev_ultrafast/model.py) | Exact request, structured criteria, validation, operation/target fan-out and transport policy |
| [agent.py](https://github.com/browser-use/jev-ultrafast/blob/452c1ad2dd628008f1d5608f28158d76e49e6cc0/jev_ultrafast/agent.py) | Observation freshness, consumed decisions, history and loop bounds |
| [snapshot.js](https://github.com/browser-use/jev-ultrafast/blob/452c1ad2dd628008f1d5608f28158d76e49e6cc0/jev_ultrafast/snapshot.js) | Indexed observed controls and sensitive-input exclusions |
| [test_agent.py](https://github.com/browser-use/jev-ultrafast/blob/452c1ad2dd628008f1d5608f28158d76e49e6cc0/tests/test_agent.py) | Selected-target-only execution checks |

## Local conventions and build

The user-supplied octocode-skills authoring/review routes define folder structure, standalone execution and navigation checks. The existing Octocode `packages/octocode-config` build artifact, from the installed octocode-scraping skill, is vendored unchanged as `scripts/octocode-config.mjs`; `scripts/jev.mjs` imports its home resolver, config parser and env loader. Jev-specific keys are this client's extension rather than a change to the central config schema.

The Rust client uses [ureq 3.4.2](https://docs.rs/ureq/3.4.2/ureq/) with Rustls HTTPS, serde_json for JSON, and httpdate for retry dates. `Cargo.lock` pins transitive versions; `Cargo.toml` defines the source build. Source was implemented for this skill; the browser agent's source is not copied into it.
