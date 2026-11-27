Load when deciding what to retrieve next, when to stop reading, or how to page.

# Evidence loop and paging

## Evidence-driven loop

Avoid rigid rules such as "always read the first ten files" or "always retrieve five pages". Let the agent decide what it needs, within bounded resources:

1. **Define the question:** which decision or claim needs evidence?
2. **Inventory:** what is already verified?
3. **Find gaps:** which missing fact could change the answer?
4. **Pick the cheapest adequate action:** exact lookup, metadata search, selective read or experiment.
5. **Update evidence:** give each finding a [grounding label](grounding.md#grounding).
6. **Judge sufficiency:** can the next action be justified?
7. **Continue or stop:** retrieve more only for a material gap.

## Pagination

1. Start with a small, relevant page.
2. Prefer server-side filters and sort order.
3. Copy the emitted cursor or continuation unchanged and treat it as opaque; keep the snapshot or revision identity it carries. Do not invent a different pagination field or offset.
4. Continue only while relevant evidence is still missing.
5. Deduplicate across pages; a repeated cursor is a cycle, not progress.
6. Stop paging at the caller's authorized budget (time, tokens, requests) and report the unread scope; a page count alone is not a reason to stop.
7. Never claim an exhaustive search unless the required pages or scope were examined. The first page is not the dataset.

Producer-side rules are in [tools](tool-results.md#pagination-contract).
