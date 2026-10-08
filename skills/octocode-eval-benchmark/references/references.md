# Method sources

Load when you audit provenance or revisit a method. Checked 2026-09-24. One primary source per page; these sources inform the controls, none prescribes this exact protocol.

| Page | Primary source | Finding applied |
|---|---|---|
| `clean-lab.md`, `graders.md` | [Anthropic: Demystifying evals for AI agents (2026)](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents) | Isolated trials, outcome checks, task fairness, grader calibration, repeated-trial reliability |
| `eval-harness.md` | [OpenAI: Evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices) | Task-specific evaluation, representative data, human calibration, continuous evaluation |
| `llm-judge.md` | [Zheng et al.: Judging LLM-as-a-Judge (2023)](https://arxiv.org/abs/2306.05685) | Position, verbosity, self-enhancement biases; agreement depends on the setting, so do not transfer a paper's agreement rate to a new domain |
| `held-out-and-guards.md` | [Dwork et al.: Holdout reuse (2015)](https://arxiv.org/abs/1506.02629) | Adaptive holdout reuse can overfit; this skill uses a simpler sealed final set, not their reusable-holdout algorithm |
| `failure-repair.md` | [OpenAI: Separating signal from noise in coding evaluations (2026)](https://openai.com/index/separating-signal-from-noise-coding-evaluations/) | Ambiguous tasks and strict or low-coverage tests can invalidate scores; check task and grader before blaming the solver |
| `benchmarks/README.md` | [Inspect: Tasks](https://inspect.aisi.org.uk/tasks.html) | Dataset, solver, scorer separation; inspect real exports, do not assume target or metadata fields are private |

Role boundaries and reporting fields are engineering recommendations: validate them in the experiment host.
