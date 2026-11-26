Load when labeling claims, tying an answer to evidence, or reducing unsupported statements.

# Grounding

Tie each important claim to an inspected source, test or observation ([match proof to the claim](evidence.md#match-proof-to-the-claim)); check freshness and contradictions. Schema/type checks establish structure; execution establishes behavior. When a wrong claim would trigger an irreversible action or reach users, add one check that does not depend on the draft (a test, a compiler, a second source) and name it in the answer; label what the evidence cannot decide Unknown. With no independent check available, label the claim inferred and name what would confirm it.

| Label | Meaning |
|---|---|
| Confirmed | Directly supported by inspected code, a trusted source, a test or an observation |
| Inferred | Reasonable interpretation of evidence, not directly verified |
| Unknown | Insufficient evidence; investigate or qualify |

- Never claim a test passed if it did not run, or that all references were inspected when search coverage was partial.
- An LLM judge helps but does not replace compilation, integration tests, runtime evidence or authoritative data.
- Do not let an inference become a fact through summaries or handoffs (evidence laundering).

## Reduce unsupported claims

No technique removes hallucination; each lowers it under conditions, so measure wrong answers, not only accuracy.

| Technique | Do | Evidence and limit |
|---|---|---|
| Allow abstention | Make "not found" or "unknown" an accepted answer, and score evals so a wrong answer costs more than abstaining (for example +1 correct, 0 abstained, −1 wrong) | On one benchmark the model that abstained on 52% of questions was wrong on 26%, and the one that abstained on 1% was wrong on 75%, at similar accuracy. A prompt alone leaves gaps: small models still answered 41.6% of misleading questions |
| Quote first | Have the model extract word-for-word quotes, answer only from them and retract a claim with no quote | Anthropic guidance; no published measurement |
| Check each citation against its source | Confirm in code that the quote is on the cited page, then that it supports the claim | Across 14 models over 94% of cited links were valid and over 80% relevant, yet 39–77% of the claims were accurate |
| Ground each step in an observation | Take facts from a tool result, retrieval or execution, and filter what enters context ([retrieval](retrieval.md)) | Tool-grounded traces had 6% hallucinated false positives against 14% for reasoning alone (2022 model); irrelevant retrieved documents lower accuracy, and fact-check accuracy fell about 42% as tool calls grew from 2 to 150 |
| Verify independently | Ask verification questions in a context that cannot see the draft, or use an external checker: a test, a compiler or a source. A second model counts only when it gets the evidence and not the draft | Factored verification raised FactScore 55.9 → 71.4 (2023 model). Self-critique with no new evidence does not help and can hurt: GPT-4 fell 95.5 → 89.0 on GSM8K. Debate did no better than self-consistency at equal cost |

A standing "double-check your work" instruction makes the newest models over-verify (Anthropic): ask for one named check instead. Reasoning techniques are in [reasoning](reasoning.md).

## Sources

- [Anthropic: reduce hallucinations](https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/reduce-hallucinations); [OpenAI: why language models hallucinate](https://openai.com/index/why-language-models-hallucinate/)
- Abstention: [AbstentionBench](https://arxiv.org/abs/2506.09038), [Setiawan](https://arxiv.org/abs/2608.22228); citations: [Onweller et al.](https://arxiv.org/abs/2605.06635)
- Verification: [Chain-of-Verification](https://arxiv.org/abs/2309.11495), [Huang et al.](https://arxiv.org/abs/2310.01798), [Stechly et al.](https://arxiv.org/abs/2402.08115); tools: [ReAct](https://arxiv.org/abs/2210.03629), [Yoran et al.](https://arxiv.org/html/2310.01558v2)
