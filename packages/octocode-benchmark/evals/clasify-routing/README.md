# clasify-routing — naming-hypothesis routing eval

Focused held-out eval for the `semanticAssess` → `clasify` public rename. It
measures whether the new name improves an agent's **first-tool selection**, not
whether the contract is well-formed (that is proven by the core, native, MCP,
and CLI test suites).

## Hypothesis

Renaming the bounded-judgment tool to `clasify` makes agents:

1. route to it **exactly** when a bounded Noul / Choice / Score judgment over
   unread or supplied evidence changes the next action, and
2. **not** route to it for exact lookup, deterministic checks, required proof, or
   free-form summarization,

**without** increasing unnecessary provider-backed classification calls.

The rename is behaviorally successful only if correct first-tool selection
improves or stays equal while unnecessary `clasify` calls do not rise.

## Corpus (`cases.mjs`)

- **positive** — the first tool should be `clasify` (unread-candidate Noul,
  Choice scout, Score rank, and supplied-draft self-review).
- **negative** — the first tool should **not** be `clasify` (exact read,
  deterministic membership, required proof, free-form summary).
- **availability** — with and without `OCTOCODE_CLASSIFICATION_API`.

`reference` fields are grader-only and must never enter a model prompt.

## Method

1. **Freeze the baseline before changing the evaluated catalog.** Capture agent
   first-tool selection on this corpus against the **pre-rename** catalog (tool
   advertised as `semanticAssess`, its old description) as the A arm. The rename
   is a hard cutover, so reconstruct the old catalog from the pre-rename core
   revision for the A arm only — do not reintroduce `semanticAssess` into the
   shipped catalog.
2. Run the **post-rename** catalog (`clasify`) as the B arm on the same corpus
   and the same models.
3. Grade first-tool selection per case: positive → chose `clasify`; negative →
   did **not** choose `clasify`; availability → respected the provider-key gate.
4. Compare B vs A: report Δ correct-first-tool and Δ unnecessary `clasify` calls
   (negative cases that wrongly routed to `clasify`).

## Verdict status

**Pending execution.** This directory ships the frozen corpus and method. The
A/B selection measurement requires a harness run (agent models + budget) and a
reconstructed pre-rename A-arm catalog; it has not been run here. Record the Δ
correct-first-tool and Δ unnecessary-call numbers in the implementation receipt
once the run completes.

`selftest.mjs` validates only the corpus shape and the contract-integrity
precondition (the live catalog advertises `clasify` and no `semanticAssess`); it
does not measure agent selection.
