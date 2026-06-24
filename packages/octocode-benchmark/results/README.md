# Benchmark Results

This folder holds the output of benchmark runs — separate from the benchmark scripts and fixtures in `benchmark/`.

## Structure

```
results/
  README.md               ← this file
  ci/
    latest.md             ← last CI suite run (matrix + AST + LSP + minify + cli)
  ast-grep/
    comparison.md         ← Octocode vs ast-grep timing table (latest)
    summary.md            ← run summary (last ast:compare:upstream)
  repo/
    react/results.md      ← facebook/react probe results (yarn repo:bench)
    tokio/results.md      ← tokio-rs/tokio probe results
    spring-boot/results.md
    chromium/results.md
    nextjs/results.md
```

For full timestamped run history (including `manifest.json`, `commands.ndjson`, `ratings.json`, raw output), see `output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/`.

## Regenerate

```bash
# CI suite (matrix + AST + LSP + minify + cli:check)
yarn benchmark

# ast-grep timing comparison
yarn ast:compare:upstream

# Cross-repo real-world probes
yarn repo:clone   # one-time clone
yarn repo:bench   # writes results/repo/<name>/results.md
```

See [BENCHMARK.md](../BENCHMARK.md) for the full benchmark guide.
