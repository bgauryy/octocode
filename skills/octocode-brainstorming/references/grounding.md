# Grounding & references

Load when you need to justify or trace the method/tooling claims in `SKILL.md`. The method here is not improvised — each pillar maps to an established source, cited so the instructions stay falsifiable.

## Method

- *Divergent vs. convergent thinking as distinct constructs* — J.P. Guilford's 1950 APA presidential address ("Creativity") launched modern creativity psychology and the divergent/convergent distinction.
  Guilford established the constructs, not a "never mix the phases" rule. [Stanford Encyclopedia of Philosophy — Creativity](https://plato.stanford.edu/entries/creativity/) · [Britannica — Divergent thinking](https://www.britannica.com/science/creativity/Divergent-thinking)
- *Diverge-then-converge as separate phases, never mixed* — this phase-separation discipline is facilitation/Creative Problem Solving (CPS) practice descending from Osborn-Parnes, not a direct Guilford theorem.
  Keep the two attributions distinct when citing this method.
- *Defer judgment / quantity-first while diverging* — Alex Osborn's brainstorming rules, *Applied Imagination* (1953). Attribution and date are the standard bibliographic reference via secondary sources.
  A primary/publisher-adjacent page was not independently reachable as of 2026-07-11 (treat 1953 as standard, not independently re-confirmed here). [Brainstorming — Wikipedia](https://en.wikipedia.org/wiki/Brainstorming)
- *Combine/shift lens (SCAMPER)* — Bob Eberle, *SCAMPER: Games for Imagination Development* (1971), systematizing Osborn's idea-spurring checklist. [SCAMPER — Wikipedia](https://en.wikipedia.org/wiki/SCAMPER)

### Modern LLM-era precedent for the stress-test lenses

`debate.md`'s fixed Critical Architect / Visionary Entrepreneur / Product lenses are structured multi-perspective critique, not consensus-seeking debate. The closest citable 2023 precedents, and how this skill differs from each:

- Du et al., *Improving Factuality and Reasoning in Language Models through Multiagent Debate* — multiple LLM instances critique each other over rounds and converge on one shared answer; optimizes agreement/factuality. [arXiv:2305.14325](https://arxiv.org/abs/2305.14325)
- Wang et al., *Unleashing the Emergent Cognitive Synergy in Large Language Models: A Task-Solving Agent through Multi-Persona Self-Collaboration* (SPP) — one model simulates multiple input-dependent personas across turns, including creative-writing tasks. [arXiv:2307.05300](https://arxiv.org/abs/2307.05300)
- This skill's lenses are closer to persistent, named stakeholder critique (design/business-case review) than either paper's consensus-oriented process.
  The three roles are fixed and can stay in genuine, unresolved tension (see `debate.md`'s Conceded/contested step), rather than converging to one shared verdict.

## Tooling (web engines)

Flags and limits below reflect the live API contracts (verified 2026-06-28):

- Tavily `/search`: `search_depth` (`basic`/`advanced`/`fast`/`ultra-fast`), `topic` (general/news/finance), `time_range`, `max_results` (0–20), `include_domains`/`exclude_domains`, `auto_parameters`, `start_date`/`end_date`. [Tavily API reference](https://docs.tavily.com/documentation/api-reference/endpoint/search)
- Serper `/search`: `q`, `gl`, `hl`, `num`, `page`, `tbs` (recency via `qdr:d|w|m|y`), `autocorrect`, `location`. [serper.dev](https://serper.dev/) · [Serper params (LiteLLM)](https://docs.litellm.ai/docs/search/serper)
- Exa `/search`: `query`, `type` (`auto`/`neural`/`keyword`/`fast`), `numResults` (1–100), `category` (e.g. `research paper`/`news`/`github`/`company`/`pdf`), `includeDomains`/`excludeDomains`, `startPublishedDate`/`endPublishedDate`, `contents.highlights`/`contents.text`. Auth via `x-api-key` header, not `Authorization: Bearer`. [Exa API reference](https://docs.exa.ai/reference/search)
