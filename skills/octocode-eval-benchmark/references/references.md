# Method sources
Load when you audit provenance or revisit a method. Checked 2026-09-24. These sources inform the controls; no source prescribes this exact protocol.

| Primary source | Supported finding | Application here |
|---|---|---|
| [Anthropic: Demystifying evals for AI agents (2026)](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents) | Isolated trials, outcome checks, task fairness, grader calibration, and repeated-trial reliability | `clean-lab.md`, `graders.md`; no hidden requirements |
| [OpenAI: Evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices) | Task-specific evaluation, representative data, human calibration, and continuous evaluation | `eval-harness.md`, `llm-judge.md` |
| [Zheng et al.: Judging LLM-as-a-Judge (2023)](https://arxiv.org/abs/2306.05685) | Position, verbosity, and self-enhancement biases; agreement depends on the studied setting | `llm-judge.md`; do not transfer a paper's agreement rate to a new domain |
| [Dwork et al.: Generalization in Adaptive Data Analysis and Holdout Reuse (2015)](https://arxiv.org/abs/1506.02629) | Adaptive holdout reuse can overfit; valid reuse requires specific statistical machinery | `held-out-and-guards.md`; a sealed final set is our simpler operational choice, not their formal reusable-holdout algorithm |
| [Optimization-based Prompt Injection Attack to LLM-as-a-Judge (2024)](https://arxiv.org/abs/2403.17710) | Candidate text can attack model judges | `llm-judge.md`; delimiters alone do not establish robustness |
| [Anthropic: Eval awareness in BrowseComp (2026)](https://www.anthropic.com/engineering/eval-awareness-browsecomp) | An agent can identify a benchmark and retrieve answers during a run | `clean-lab.md`; network/retrieval exposure is part of contamination review |
| [Inspect: Tasks](https://inspect.aisi.org.uk/tasks.html) and [Sandboxing](https://inspect.aisi.org.uk/sandboxing.html) | Dataset/solver/scorer separation, per-sample environments, and metadata exposed to sandbox configuration | `benchmarks/README.md`; inspect real exports rather than assuming target/metadata fields are private |
| [OpenAI: Separating signal from noise in coding evaluations (2026)](https://openai.com/index/separating-signal-from-noise-coding-evaluations/) | Ambiguous/misleading tasks and overly strict or low-coverage tests can invalidate scores | `failure-repair.md`; check task and grader before blaming the solver |
| [Anthropic: Infrastructure noise in agentic coding evals (2026)](https://www.anthropic.com/engineering/infrastructure-noise) | Resource allocation and enforcement affect both reliability and achievable capability | `failure-repair.md`; pin guarantees and hard limits and rerun matched arms |

Do not equate judge consensus with correctness, privacy with lack of contamination, or a statistical interval with representative coverage. Role boundaries and reporting fields are engineering recommendations; validate them in the experiment host.
