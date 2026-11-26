Load when deciding whether to call a tool or which retrieval technique fits.

# Retrieval

Principle: discover broadly, inspect narrowly, expand only when evidence requires it.

## When to call a tool

| Situation | Action |
|---|---|
| Existing verified context supports the answer | Answer directly |
| Need current external facts | Search or retrieve |
| Know the exact resource identifier | Read that resource |
| Know the topic, not the location | Search metadata or an index |
| Need only a subset | Filter and paginate |
| Several reads with no shared state and no writes | Run them in parallel |
| Need a reusable procedure | Load the matching skill |
| Need substantial isolated analysis | Delegate to a subagent |
| Need to modify external state | Validate permissions, then execute |
| Consequential or irreversible action | Get the applicable approval |
| A tool fails repeatedly | Diagnose, use a bounded fallback, or stop |

When to stop retrieving is in the [evidence loop](evidence-loop.md#evidence-driven-loop).

## Retrieval techniques

| Know or need | Use |
|---|---|
| Know the symbol, identifier or error text | Exact-match search |
| Know only the concept | Semantic search, or hybrid search combining keyword and semantic retrieval |
| Need the relevant files before reading contents | Metadata-first retrieval |
| Large repository | Hierarchical search: repository → module → symbol |
| Need a subset of a file | Range read of the relevant lines or sections |
| Need a few fields of a record | Projection: fetch only the required fields |
| A match needs its surroundings | Incremental expansion around the match |
| Large result set | Cursor pagination ([evidence loop](evidence-loop.md#pagination)) |
