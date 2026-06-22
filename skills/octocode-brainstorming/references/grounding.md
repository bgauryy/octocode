# Grounding & references

Load when you need to justify or trace the method/tooling claims in `SKILL.md`. The method here is not improvised — each pillar maps to an established source, cited so the instructions stay falsifiable.

## Method

- *Diverge-then-converge, never mixed* — divergent vs. convergent thinking, originated by J.P. Guilford (1950s). [Divergent thinking — Wikipedia](https://en.wikipedia.org/wiki/Divergent_thinking) · [Divergent vs. Convergent Thinking — Interaction Design Foundation](https://www.interaction-design.org/literature/topics/divergent-thinking)
- *Defer judgment / quantity-first while diverging* — Alex Osborn's brainstorming rules, *Applied Imagination* (1953). [Brainstorming — Wikipedia](https://en.wikipedia.org/wiki/Brainstorming)
- *Combine/shift lens (SCAMPER)* — Bob Eberle, *SCAMPER: Games for Imagination Development* (1971), systematizing Osborn's idea-spurring checklist. [SCAMPER — Wikipedia](https://en.wikipedia.org/wiki/SCAMPER)

## Tooling (web engines)

Flags and limits below reflect the live API contracts (verified 2026-06-22):

- Tavily `/search`: `search_depth`, `topic` (general/news/finance), `time_range`, `max_results` (0–20), `include_domains`/`exclude_domains`, `auto_parameters`, `start_date`/`end_date`. [Tavily API reference](https://docs.tavily.com/documentation/api-reference/endpoint/search)
- Serper `/search`: `q`, `gl`, `hl`, `num`, `page`, `tbs` (recency via `qdr:d|w|m|y`), `autocorrect`, `location`. [serper.dev](https://serper.dev/) · [Serper params (LiteLLM)](https://docs.litellm.ai/docs/search/serper)
