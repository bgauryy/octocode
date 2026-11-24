# Graph search questions

Fifty labelled questions for `octocode-graph` `search`, scored by a script rather than a judge. Unlike the research sets,
each question carries its answer: [search-eval.json](search-eval.json) names the graph it runs on, the query an agent
types, and every `file#symbol` that answers it, with the `path:line` where the code was read. The answers were taken from
the code, never from the tool under test.

| Kind | Questions | What it asks |
|---|---:|---|
| exact | 8 | the whole name (`cycleGroups`, `make_password`) |
| partial | 7 | some of the name's words (`shortest ring`, `msgpack writer`) |
| intent | 30 | what the code does, in words its name does not hold (`where are dependency cycles counted`, `which code retries failed requests`) |
| type | 5 | parameter and return types; `typed` gives the same question as `--param`/`--returns` filters |

The graphs: this repository's `packages/octocode-graph` (13) and `packages/octocode-native/crates/tool-graph` (12), and
two pinned clones from [local/README.md](../../local/README.md), Django (12) and TypeScript's `packages` (13).
`search-eval.json` says how each was built (`--lsp off`).

## Score

```sh
node packages/octocode-graph/scripts/search-eval.mjs \
  --graph octocode-graph=<dir> --graph tool-graph=<dir> --graph django=<dir> --graph typescript-packages=<dir> \
  [--flags "<search flags>"] [--engine <another query-api.ts>] [--save <file.json>]
```

A row answers when its symbol is an expected one (an overload's `@N` suffix ignored). Recall@1 and recall@10 count the
questions answered at those ranks; MRR is the mean reciprocal rank of the first answering row among the 20 shown. A
question whose expected symbol is no longer in the graph stops the run: it is stale, not a miss. `typed` questions run
too when the graph's symbols record `params` or `returns`.

## Results

Stored beside the set, each with one row per question. The graphs were built once, with the engine that records
parameter and return types, and every run scored the same four.

| Run | R@1 | R@10 | MRR | Intent R@10 | Type R@10 | Typed filters R@1 / R@10 | File |
|---|---:|---:|---:|---:|---:|---:|---|
| v20 search: name tiers only | 26 | 30 | 28.0 | 0 | 0 | 0 / 0 | [baseline.json](baseline.json) |
| BM25F over data.json alone (no sidecar) | 40 | 68 | 51.2 | 50.0 | 80 | not run | not stored |
| BM25F with doc comments and every declaration (default) | 54 | 76 | 61.3 | 63.3 | 80 | 60 / 80 | [bm25f.json](bm25f.json) |
| `--expand` (removed): related words and graph neighbours | 52 | 78 | 60.3 | 66.7 | 80 | 60 / 80 | [expand.json](expand.json) |

Every exact-name question ranks its symbol first in every run. `--expand` adds 2 points of recall@10 over BM25F (one
question), under the 5 points the RFC's D7 asks, so it was removed and its scores stay here as the evidence; the same held on graphs without
signatures (70 → 72). Typed filters run the `typed` form (`--returns QueryResult`) of the 5 type questions.
