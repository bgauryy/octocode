Load when choosing, justifying or questioning a thinking method this skill prescribes, or when re-checking the evidence behind it.

# Method evidence

Each method here, in [self-evaluation, debate and model mix](method-evidence-judging.md) and in [consider the opposite, premortem and competing hypotheses](method-evidence-challenge.md), has a rule in this skill, a status by the [proof bar](proof-bar.md#contradict-and-prove) and its sources. Most studies used product or research ideation with GPT-4-era models, or human groups: a rule built from them is an untested prescription for this skill until a run measures it ([calibrated confidence](proof-bar.md#calibrated-confidence)).

Every quote is in [method-evidence.jsonl](method-evidence.jsonl), one row per source passage, grouped by the `decides` id in each heading. Re-check them all, for example before changing a rule:

```bash
node <skill-folder>/scripts/verify-claims.mjs --ledger <skill-folder>/references/method-evidence.jsonl --out <run>/method-evidence.verified.jsonl
```

## Independent passes

`independent-passes` · **proven** for product and research ideation · rule: [step 3](../SKILL.md#steps), [diverger brief](briefs.md#diverger-brief)

An LLM's option set clusters: unprompted GPT-4 idea pools are less diverse than human pools, early outputs anchor later ones, and over-generating mostly yields duplicates. Step-by-step (chain-of-thought) generation, varied personas or constraints, and pooling across vendors close most of the gap; personas alone widen coverage but deepen fixation.

- [Meincke, Mollick, Terwiesch 2024, Prompting Diverse Ideas](https://arxiv.org/abs/2402.01727): GPT-4 pools less diverse than human groups; chain-of-thought gave the highest diversity.
- [Deng, Brucks, Toubia 2026, A Cognitive Framework for Idea Diversity in LLMs](https://arxiv.org/abs/2602.20408): LLMs fixate on early outputs; personas plus chain-of-thought gave the highest diversity.
- [Si, Yang, Hashimoto 2024, Can LLMs Generate Novel Research Ideas?](https://arxiv.org/html/2409.04109v1): about 5% of generated seed ideas were non-duplicates, even when told to avoid repeats.
- [Terwiesch et al. 2026, AI and Its Impact on Creativity and Diversity](https://arxiv.org/abs/2607.27553): AI ideas less diverse as a set across prior studies; pooling across vendors, chain-of-thought and heterogeneous personas or constraints restore diversity.
- Limit: the same 2026 study found higher average idea quality for LLM ideas; diversity is not decision quality.

## Generate alone first

`generate-alone-first` · **proven** for face-to-face human groups · rule: [step 3](../SKILL.md#steps), [independence](judging.md#independence-and-incentives)

People who generate alone and then pool produce more and better ideas than groups brainstorming together, and pick their best ideas better.

- [Mullen, Johnson, Salas 1991, meta-analysis](https://www.tandfonline.com/doi/abs/10.1207/s15324834basp1201_1): brainstorming groups less productive than nominal groups in quantity and quality.
- [Girotra, Terwiesch, Ulrich 2010, Idea Generation and the Quality of the Best Idea](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=1082392): the alone-then-together process beat team work on quantity, quality and choosing the best.
- Against: [Small Group Research 2013, group size in electronic brainstorming](https://doi.org/10.1177/1046496413479674): large electronic groups can out-generate nominal groups.
- Limit: human studies; the transfer to model passes is inference, supported by the fixation finding above.

## Not yet researched

Six Thinking Hats, morphological analysis, far analogies, reference-class forecasting, and devil's advocacy versus dialectical inquiry ([Schwenk 1990 meta-analysis](https://doi.org/10.1016/0749-5978%2890%2990051-a), abstract not read). None is a rule here; research one before adding it.
