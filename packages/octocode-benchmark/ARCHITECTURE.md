# Benchmark architecture

`@octocodeai/octocode-benchmark` is a private, source-only evaluation workspace. It measures research behavior and resource use; it is not a production dependency and must not become a second implementation of tool policy.

## Campaign boundaries

Each campaign owns its corpus, protocol, arms, receipts, graders, and report format under one directory. Results from different campaigns are not directly comparable unless a shared protocol explicitly says they are.

- `compare/terra-v3` is the current locked-corpus tool comparison.
- `compare/advanced-research-v1` is a separate local/GitHub diagnostic.
- `compare/github-questions` and `results/` retain the historical GitHub campaign.
- `evals/` contains focused deterministic regression gates.

## Data flow

```text
frozen cases + corpus + arm config
              │
              ▼
       preflight / eligibility
              │
              ▼
      isolated arm execution
              │
              ▼
 raw receipts + resource measures
              │
              ▼
 deterministic grading / blind review
              │
              ▼
          campaign report
```

## Invariants

- Freeze cases and scoring before evaluating the change under test.
- Keep raw observations separate from derived scores and narrative conclusions.
- Record tool versions, source fingerprints, corpus revisions, and exclusions.
- Never silently combine character, token, time, or quality metrics from incompatible campaigns.
- Repository-layout probes are allowed because this package is internal, but preflight must fail clearly when required artifacts are absent.
- Benchmark findings inform a keep/discard decision; they do not override production tests or security gates.
