# Typed source profile

Load when the host has already selected one or more source units and needs reusable semantic judgments without first loading their bytes into the host-model context. This is not scout read-prioritization: a profile describes supplied sources; it does not decide which sources to read.

## Flow

`PATH OR INLINE TEXT → SANDBOX/REDACT/BOUND → ONE STATE PER SOURCE → BATCH ASPECT QUESTIONS → PARALLEL SOURCE REQUESTS → TYPED PROFILE`

`scripts/profile.mjs` accepts 1–8 inputs. Each input supplies exactly one root-relative local `path` (optionally an exact `lines` range) or inline `content`. Local reading and redaction happen in the runner. Oversized content fails explicitly instead of being silently summarized; select a line range or deliberately raise `maxChars`.

All aspects for one source are sent together in one Jev request because they share state and Jev evaluates questions independently. Separate sources run concurrently. Do not make one request per aspect: that retransmits the same content. When a judgment inherently compares files, compose those files into one bounded inline source so the relationship remains in one state.

Use the primitive matching the answer:

- `score` for an ordered spectrum with 2–10 concrete situation levels;
- `noul` for whether one condition holds;
- `choice` for one member of a supplied unordered set.

Jev returns typed probabilities, not a summary or explanation. Code owns weights, thresholds, routing, and display. A profile is always provisional and never citable evidence; reopen the reported source anchor before explaining or asserting code behavior.

## Run

Build input from `assets/profile-input.schema.json`, then:

```sh
node scripts/profile.mjs --input profile.json --dry-run node scripts/profile.mjs --input profile.json node --test scripts/profile.test.mjs  # deterministic safety, typing, batching, and concurrency checks
```

Example:

```json
{ "goal": "Build a reusable semantic profile of the selected source units.", "root": ".", "inputs": [ { "id": "runtime", "path": "src/runtime.ts" }, { "id": "adapter", "content": "export const run = core.run", "source": "generated excerpt" } ], "aspects": [ { "key": "responsibilityCohesion", "type": "score", "instructions": "How cohesive are this source unit's responsibilities?", "criteria": [ "Unrelated responsibilities", "Several related responsibilities", "One coherent responsibility" ] }, { "key": "ownsBehavior", "type": "noul", "instructions": "Does this source unit directly own runtime behavior?", "criteria": { "true": "Directly implements behavior", "false": "Only describes, imports, or delegates behavior" } } ] }
```

`candidate bytes stay off host context` means only that the host model need not read them first. Redacted content is still sent to the configured external Jev service. Do not profile source that policy forbids disclosing.

## Scope and quality status

This is a skill-level prototype, not a native public tool contract. Deterministic tests cover sandboxing, redaction, bounds, source identity, one-file support, mixed primitives, and batching topology. Semantic profile accuracy is not yet benchmarked; define labeled dimensions and held-out cases before automating consequential behavior from profile thresholds.
