# Real-Code Minifier Benchmark

This directory records before/after excerpts and byte-cut metrics for real code
samples. Full third-party source files are intentionally not vendored here; use
the generator to recreate reports from a local corpus.

## Summary

- Samples covered: 46
- Symbol skeletons returned: 31/31
- Average byte cuts: content-view 26.8%, apply 30.9%, async 30.9%

## Markdown

Real sample: `md/rust-readme.md`

| Output            | Bytes |   Cut |
| ----------------- | ----: | ----: |
| input             |  3304 |     - |
| content-view      |  3264 |  1.2% |
| applyMinification |  3264 |  1.2% |
| sync minify       |  3264 |  1.2% |
| async minify      |  3264 |  1.2% |
| symbols           |  1961 | 40.6% |

See `benchmark/md/README.md` for the real before/after excerpts and Markdown
symbol outline.

## Regenerate

```bash
yarn build
node benchmark/generate-real-code-report.mjs /path/to/real/corpus
```
