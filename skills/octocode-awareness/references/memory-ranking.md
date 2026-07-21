# Memory Ranking

Read this when recall returns too few rows, surprising order, low confidence, or when semantic ranking is being configured.

## Modes

- Default `--sort smart|score` blends lexical relevance, importance, recency, and access.
- `--sort importance|recent|accessed` isolates one ordering signal.
- `--explain` adds score components so the agent can justify selection.
- `--smart` safely broadens an under-filled strict query by lowering minimum importance and then dropping label/tag filters.
- `--as-of <ISO>` evaluates memory validity at a prior time.

## Semantic Recall

`--semantic` reranks only when embeddings exist. Configure `OCTOCODE_EMBED_CMD` as a command that reads text on stdin and prints JSON:

```json
{"embedding":[0.1,0.2],"model":"host-model"}
```

With the command set, `memory record` stores vectors and semantic recall ranks by cosine similarity. When unset/failing, CLI warns and falls back to lexical/salience mode. Pi needs the same host env/API; library callers may use `storeEmbedding` and `searchByEmbedding`.

## Judgment

1. Inspect mode and `score_components` before trusting order.
2. For zero/weak results, vary vocabulary, remove narrow filters, or use `--smart`.
3. Increase `--limit` only when comparison needs more candidates; compact context is the default.
4. Treat semantic similarity as retrieval help, not truth.
5. Verify selected claims against current source/tests and repair stale memory through the workflow in `references/memory-recall.md`.
