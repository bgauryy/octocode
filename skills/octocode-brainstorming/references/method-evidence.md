Load when choosing, justifying or questioning a thinking method this skill prescribes, or when re-checking the evidence behind it.

# Method evidence

Each method below has a rule in this skill, a status by the [proof bar](critical-thinking.md#contradict-and-prove) and its sources. Most studies used product or research ideation with GPT-4-era models, or human groups: a rule built from them is an untested prescription for this skill until a run measures it ([calibrated confidence](critical-thinking.md#calibrated-confidence)).

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

`generate-alone-first` · **proven** for face-to-face human groups · rule: [step 3](../SKILL.md#steps), [independence](critical-thinking.md#independence-and-incentives)

People who generate alone and then pool produce more and better ideas than groups brainstorming together, and pick their best ideas better.

- [Mullen, Johnson, Salas 1991, meta-analysis](https://www.tandfonline.com/doi/abs/10.1207/s15324834basp1201_1): brainstorming groups less productive than nominal groups in quantity and quality.
- [Girotra, Terwiesch, Ulrich 2010, Idea Generation and the Quality of the Best Idea](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=1082392): the alone-then-together process beat team work on quantity, quality and choosing the best.
- Against: [Small Group Research 2013, group size in electronic brainstorming](https://doi.org/10.1177/1046496413479674): large electronic groups can out-generate nominal groups.
- Limit: human studies; the transfer to model passes is inference, supported by the fixation finding above.

## Self-evaluation

`self-evaluation` · **proven** for judging reasoning and ranking ideas · rule: [your own judgment](critical-thinking.md#your-own-judgment-is-not-evidence), [workers](phases.md#workers)

Without outside feedback, an LLM's self-correction of reasoning fails and can degrade answers, and its ranking of its own ideas is unreliable. LLM ideas were judged more novel but slightly less feasible than experts' ideas.

- [Huang et al. 2024, Large Language Models Cannot Self-Correct Reasoning Yet](https://arxiv.org/abs/2310.01798)
- [Si, Yang, Hashimoto 2024](https://arxiv.org/abs/2409.04109): failures of LLM self-evaluation; more novel, weaker on feasibility.
- Against: [Madaan et al. 2023, Self-Refine](https://arxiv.org/abs/2303.17651): self-feedback made outputs preferred by about 20% on average; it improves how output reads, not whether a judgment is right.

## Debate

`debate` · **proven** · rule: [debate gate](critical-thinking.md#debate-when-and-how)

Multi-agent debate does not reliably beat cheaper single-agent methods such as self-consistency and chain-of-thought, even with more compute.

- [Smit et al., ICML 2024, Should we be going MAD?](https://proceedings.mlr.press/v235/smit24a.html)
- [arXiv 2502.08788, Stop Overvaluing Multi-Agent Debate](https://arxiv.org/abs/2502.08788): 5 methods, 9 benchmarks, 4 models.
- Against: the same ICML paper found tuned debate setups (Multi-Persona) can do better.

## Model heterogeneity

`model-heterogeneity` · **unresolved** (one source) · rule: [workers](phases.md#workers)

Mixing different models is reported to improve debate frameworks. Treat a different model for a critic or diverger as a cheap option to test, not a proven gain; pooling across vendors for ideas is under [independent passes](#independent-passes).

- [arXiv 2502.08788](https://arxiv.org/abs/2502.08788)

## Consider the opposite

`consider-the-opposite` · **proven**, with a condition · rule: [thinking with the user](critical-thinking.md#thinking-with-the-user)

Asking for a specific opposite possibility corrected bias more than asking people to be fair and unbiased, and a second estimate drawn from different knowledge reduces error. Asked for many reasons that are hard to find, people entrench their first judgment instead.

- [Lord, Lepper, Preston 1984, Considering the opposite](https://europepmc.org/article/MED/6527215)
- [Herzog, Hertwig 2009, The Wisdom of Many in One Mind](https://journals.sagepub.com/doi/abs/10.1111/j.1467-9280.2009.02271.x)
- Against: [Schwarz et al., metacognitive experiences and debiasing](https://dornsife.usc.edu/norbert-schwarz/wp-content/uploads/sites/231/2023/11/07_aep_schwarz_et_al_setting-people-straight.pdf): the strategy works only when alternatives come easily.
- For the model itself, the skill's version is evidence-based: search against the claim ([contradict and prove](critical-thinking.md#contradict-and-prove)).

## Premortem

`premortem` · **unresolved** · rule: [checks for critics](critical-thinking.md#checks-for-critics-and-the-judge)

A premortem reduced plan confidence more than other methods, measured by the method's authors; lower confidence is not shown to be better decisions. The study usually cited for "prospective hindsight" found that treating the outcome as certain, not placing it in the past, shaped the explanations. Use it to generate failure hypotheses, stated as a certain failure.

- [Veinott, Klein, Wiggins 2010, Evaluating the effectiveness of the PreMortem](https://www.researchgate.net/profile/Gary-Klein-3/publication/266485127_Evaluating_the_Effectiveness_of_the_PreMortem_Technique_on_Plan_Confidence/links/565512f708ae1ef929770299/Evaluating-the-Effectiveness-of-the-PreMortem-Technique-on-Plan-Confidence.pdf)
- [Mitchell, Russo, Pennington 1989, Back to the future](https://www.researchgate.net/publication/227768493_Back_to_the_future_Temporal_perspective_in_the_explanation_of_events)

## Competing hypotheses

`competing-hypotheses` · **proven** · rule: [hypotheses](critical-thinking.md#hypotheses-before-targeted-search)

Analysis of Competing Hypotheses, run as a full matrix procedure, did not reliably reduce confirmation bias in controlled studies and may add inconsistency. The skill keeps its core question, diagnosticity, as a check on evidence, and gets debiasing from searching against each claim.

- [Dhami, Belton, Mandel 2019](https://onlinelibrary.wiley.com/doi/abs/10.1002/acp.3550)
- [Whitesmith 2018](https://www.tandfonline.com/doi/abs/10.1080/02684527.2018.1534640)

## Not yet researched

Six Thinking Hats, morphological analysis, far analogies, reference-class forecasting, and devil's advocacy versus dialectical inquiry ([Schwenk 1990 meta-analysis](https://doi.org/10.1016/0749-5978%2890%2990051-a), abstract not read). None is a rule here; research one before adding it.
