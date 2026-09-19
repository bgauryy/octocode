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

Jev returns provisional typed judgments, not citable evidence. Code owns thresholds, AND/OR composition and routing; uncertainty or an error is not a false condition. Before asserting behavior, inspect decisive original source if not already inspected and current. A completed profile does not require another read or reasoning gate by itself.

## Run

For a compact invocation, use `scripts/ask-file.mjs --files a.ts --aspects aspects.json --context "Public input accepted by parseInput" --model jev-1.13.0`. The aspects file is the same array used by `profile.mjs`; no second question format exists. `--questions "q1 || q2"` remains the Noul shortcut. `--lines S-E` selects that range in each local file; `--output DIR` chooses the artifact directory. Output retains the typed answer, full distribution where available, resolved model, usage, anchors, and artifacts.

An aspect should state one literal condition and identify its scope. If a question presupposes a feature, provide explicit alternatives instead of forcing a yes/no answer:

```json
[
  {
    "key": "labelBehavior",
    "type": "choice",
    "instructions": "For the public input accepted by parseInput, how is an explicit source label handled on contentRef evidence? Judge acceptance of that input shape first.",
    "criteria": {
      "preserves": "Accepts contentRef evidence and retains its source label.",
      "replaces": "Accepts contentRef evidence but replaces its source label.",
      "unsupported": "Does not accept contentRef evidence.",
      "insufficient": "The supplied code does not show enough input handling to decide."
    }
  }
]
```

The optional profile `context` field supplies up to 4000 characters of shared scope and relevant facts. Questions still cannot see other answers. A dependent question requires a later request, or an explicit premise whose result code consumes only when that premise holds. Model confidence describes the distribution, not whether the source interpretation is correct.

Build input from `assets/profile-input.schema.json`, then:

```sh
node scripts/profile.mjs --input profile.json --dry-run
node scripts/profile.mjs --input profile.json
node --test scripts/profile.test.mjs
```

`candidate bytes stay off host context` means only that the host model need not read them first. Redacted content is still sent to the configured external Jev service. Do not profile source that policy forbids disclosing.

## Scope and quality status

This is a skill-level prototype, not a native public tool contract. Deterministic tests cover sandboxing, redaction, bounds, source identity, one-file support, mixed primitives, and batching topology. Semantic profile accuracy is not yet benchmarked; define labeled dimensions and held-out cases before automating consequential behavior from profile thresholds.
