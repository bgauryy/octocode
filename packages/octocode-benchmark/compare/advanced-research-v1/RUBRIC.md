# A01/A02 diagnostic rubric, v1

Frozen before the first measured runner call. Each case has ten binary outcome
checks. A check earns one point only when the answer states the correct behavior
with supporting source citations. Keyword appearance alone earns nothing. Report
unsupported claims separately; an answer claiming the opposite of a required
behavior fails that check. Source-only parent review is diagnostic, not independent
blind-judge quality. A03–A12 have no scoring oracle yet.

## A01

1. Tags are `['a', 'b', 'z']`, sorted and deduplicated.
2. owner is B; ordinary nested metadata is replaced by `{'b': 2}`, not deep-merged.
3. lc_versions retains partner `1` and replaces core with `2`.
4. Metadata also contains model `mB` and checkpoint_ns `ns`; temperature is not
   automatically copied. Metadata values already present take precedence over
   automatic promotion during ensure_config.
5. Configurable values are model `mB`, temperature `0.2`, checkpoint_ns `ns`.
6. Recursion limit stays 7 when B supplies the default; run_name stays `parent`.
7. ensure_config copies the designated top-level containers, without promising
   independent nested objects or universal deep immutability.
8. Callback replacement in patch_config removes run_name and run_id; the sequence
   consumes the root run_id at on_chain_start and assigns child callback managers.
9. Only the first sequence step receives invocation kwargs.
10. A BaseException from a step triggers parent on_chain_error then re-raises;
    on_chain_end is the success branch.

Anchors at the locked LangChain commit:
`libs/core/langchain_core/runnables/config.py:255`, `:285`, `:296`, `:379`,
`:400`, `:431`, `:450`, `:487`; `libs/core/langchain_core/runnables/base.py:3430`.

## A02

1. The second call reuses A's cached result; object B is not evaluated/applied.
2. Key fields are dir, phase, hasCustomConfig, reactProductionProfiling,
   debugPrerender, pid; presence/flags are Boolean-normalized and the JSON is hashed.
3. Object contents, silent, reporter, rawConfig and bundler are not key fields.
4. A hit sets cacheHit and invokes the newly supplied reporter with cached feature data.
5. rawConfig selects cached raw data only when requested and that data is truthy;
   otherwise it returns cached processed config.
6. The hit returns before resetting NextInstanceErrorState.nextConfig.
7. The hit bypasses fresh normalization/default/adapter work in the implementation.
8. The exported wrapper still calls warnIfReact18IsInstalled after the implementation
   returns; this does not mean an actual warning must be emitted.
9. silent=false enables timing measurement but cacheHit suppresses the timing event.
10. Object.freeze is shallow, and a proposed test varies truthy A/B while holding
    key fields constant and verifies the second value/side effects against fresh B.

Anchors at the locked Next.js commit:
`packages/next/src/server/config.ts:1813`, `:1842`, `:1853`, `:1903`, `:1945`,
`:1955`, `:1972`, `:2109`; `packages/next/src/server/config-shared.ts:2409`.

## Improvement acceptance

Primary KPI: fully evidenced behavioral checks passed per case. Efficiency is
secondary: compare provider input/output/reasoning/cached tokens only after
checking quality. Freeze prompt, budget, model, corpus and rubric per experiment.
Report all schema errors, empty calls, retries, truncated outputs and excluded trials.
Do not promote a one-pass public pilot to a release gate. Before accepting a tool
change, use at least three paired passes and independently reviewed held-out cases.
