# Retrieval and grounding

Load when deciding whether to call a tool, what to retrieve next, when to stop reading, how to page, how to label claims, or how long the answer should be.

## Contents

- [When to call a tool](#when-to-call-a-tool)
- [Retrieval techniques](#retrieval-techniques)
- [Evidence-driven loop](#evidence-driven-loop)
- [Pagination](#pagination)
- [Grounding](#grounding)
- [Answer size](#answer-size)

Principle: discover broadly, inspect narrowly, expand only when evidence requires it.

## When to call a tool

| Situation | Action |
|---|---|
| Existing verified context supports the answer | Answer directly |
| Need current external facts | Search or retrieve |
| Know the exact resource identifier | Read that resource |
| Know the topic, not the location | Search metadata or an index |
| Need only a subset | Filter and paginate |
| Several independent reads | Parallelize when safe |
| Need a reusable procedure | Load the matching skill |
| Need substantial isolated analysis | Delegate to a subagent |
| Need to modify external state | Validate permissions, then execute |
| Consequential or irreversible action | Get the applicable approval |
| Evidence is sufficient | Stop retrieving |
| A tool fails repeatedly | Diagnose, use a bounded fallback, or stop |

A tool is not called because it exists.

## Retrieval techniques

| Technique | Use |
|---|---|
| Metadata-first retrieval | Find relevant files without reading contents |
| Hierarchical search | Navigate repository → module → symbol |
| Hybrid search | Combine keyword and semantic retrieval |
| Exact-match search | Symbols, identifiers, error messages |
| Semantic search | Conceptually related material |
| Reranking | Put the most relevant results first |
| Top-K retrieval | Limit initial results |
| Cursor pagination | Browse large result sets |
| Range reading | Read the relevant lines or sections |
| Projection | Fetch only the required fields |
| Incremental expansion | Load more around relevant matches |
| Deduplication | Remove repeated hits |
| Coverage tracking | Know which questions remain open |
| Stop criteria | End retrieval when evidence suffices |

## Evidence-driven loop

Avoid rigid rules such as "always read the first ten files" or "always retrieve five pages". Let the agent decide what it needs, within bounded resources:

1. **Define the question:** which decision or claim needs evidence?
2. **Inventory:** what is already verified?
3. **Find gaps:** which missing fact could change the answer?
4. **Pick the cheapest adequate action:** exact lookup, metadata search, selective read or experiment.
5. **Update evidence:** mark findings confirmed, inferred or unresolved.
6. **Judge sufficiency:** can the next action be justified?
7. **Continue or stop:** retrieve more only for a material gap.

Heuristic for the next action, a conceptual score rather than a calibrated formula:

```text
utility = expected reduction in decision uncertainty / (latency + token cost + operational risk)
```

Sketch of adaptive retrieval; `rerank` and `sufficient` are application policies, and a real version also handles timeouts, authorization, stale hits and incomplete coverage:

```python
def retrieve_evidence(query, search, fetch):
    hits = search(query=query, limit=20, fields=["id", "title", "snippet", "score"])
    evidence, seen = [], set()
    for hit in rerank(hits, query):
        if hit["id"] in seen:
            continue
        seen.add(hit["id"])
        if sufficient(evidence, query):
            break
        evidence.append(fetch(id=hit["id"], relevant_ranges=True, max_tokens=2500))
    return evidence
```

## Pagination

1. Start with a small, relevant page.
2. Prefer server-side filters and sort order.
3. Keep stable cursors and snapshot or revision identity where supported; treat cursors as opaque.
4. Continue only while relevant evidence is still missing.
5. Deduplicate across pages; a repeated cursor is a cycle, not progress.
6. Bound total pages, time, tokens and requests.
7. Never claim an exhaustive search unless the required pages or scope were examined. The first page is not the dataset.

Producer-side rules are in [tools](tools-and-mcp.md#pagination-contract).

## Grounding

| Technique | Purpose |
|---|---|
| Grounded generation | Tie conclusions to retrieved evidence |
| Citation / provenance | Identify supporting sources |
| Claim verification | Check important factual statements |
| Abstention | Avoid unsupported conclusions |
| Uncertainty representation | Separate known from unknown |
| Tool-based verification | Validate against external state |
| Schema validation | Enforce output shape |
| Type checking | Detect invalid code or contracts |
| Execution-based testing | Validate actual behavior |
| Independent verification | Catch some generation errors |
| Contradiction checking | Detect inconsistent evidence |
| Freshness validation | Avoid outdated conclusions |
| Adversarial testing | Expose brittle assumptions |
| Faithfulness evaluation | Measure whether statements follow evidence |

| Label | Meaning |
|---|---|
| Confirmed | Directly supported by inspected code, a trusted source, a test or an observation |
| Inferred | Reasonable interpretation of evidence, not directly verified |
| Unknown | Insufficient evidence; investigate or qualify |

- Never claim a test passed if it did not run, or that all references were inspected when search coverage was partial.
- An LLM judge helps but does not replace compilation, integration tests, runtime evidence or authoritative data.
- Do not let an inference become a fact through summaries or handoffs (evidence laundering).

## Answer size

| Destination | Format |
|---|---|
| Human with a simple question | Direct, concise answer |
| Developer debugging | Root cause, evidence, fix, tests |
| Calling agent | Structured result with references |
| Build pipeline | Machine-readable status and artifacts |
| Architecture reviewer | Findings, impact, evidence, recommendations |
| Incident report | Timeline, cause, impact, corrective actions |
| Autonomous workflow | State transition, next action, blockers |
| Detailed research | Evidence-backed structured explanation |

Shorten by removing repetition and process narration, never by dropping material uncertainty or required proof.
