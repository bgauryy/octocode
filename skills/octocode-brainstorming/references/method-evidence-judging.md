Load when questioning or re-checking the evidence behind the rules on self-evaluation, debate or using a different model. How to re-check every quote: [method evidence](method-evidence.md).

# Method evidence: self-evaluation, debate and model mix

## Self-evaluation

`self-evaluation` · **proven** for judging reasoning and ranking ideas · rule: [your own judgment](proof-bar.md#your-own-judgment-is-not-evidence), [workers](workers.md#workers)

Without outside feedback, an LLM's self-correction of reasoning fails and can degrade answers, and its ranking of its own ideas is unreliable. LLM ideas were judged more novel but slightly less feasible than experts' ideas.

- [Huang et al. 2024, Large Language Models Cannot Self-Correct Reasoning Yet](https://arxiv.org/abs/2310.01798)
- [Si, Yang, Hashimoto 2024](https://arxiv.org/abs/2409.04109): failures of LLM self-evaluation; more novel, weaker on feasibility.
- Against: [Madaan et al. 2023, Self-Refine](https://arxiv.org/abs/2303.17651): outputs refined with self-feedback were preferred by humans and automatic metrics, with task performance about 20% (absolute) higher on average; it improves how output reads, not whether a judgment is right.

## Debate

`debate` · **proven** · rule: [debate gate](judging.md#debate-when-and-how)

Multi-agent debate does not reliably beat cheaper single-agent methods such as self-consistency and chain-of-thought, even with more compute.

- [Smit et al., ICML 2024, Should we be going MAD?](https://proceedings.mlr.press/v235/smit24a.html)
- [arXiv 2502.08788, Stop Overvaluing Multi-Agent Debate](https://arxiv.org/abs/2502.08788): 5 methods, 9 benchmarks, 4 models.
- Against: the same ICML paper found tuned debate setups (Multi-Persona) can do better.

## Model heterogeneity

`model-heterogeneity` · **unresolved** (one source) · rule: [workers](workers.md#workers)

Mixing different models is reported to improve debate frameworks. Treat a different model for a critic or diverger as a cheap option to test, not a proven gain; pooling across vendors for ideas is under [independent passes](method-evidence.md#independent-passes).

- [arXiv 2502.08788](https://arxiv.org/abs/2502.08788)
