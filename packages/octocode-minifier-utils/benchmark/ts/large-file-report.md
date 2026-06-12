# TypeScript (`.ts`) — Large-File Benchmark

**Source:** 322 lines / 10,126 bytes — Async HTTP client with generics, retry, EventEmitter

**Agent rating: 9.5/10 (excellent)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,126 B | — | — |
| content-view | 7,501 B | **−25.9%** | 6.58 ms |
| applyMinification | 3,911 B | **−61.4%** | 407.62 ms |
| minifyContentSync | 3,911 B | **−61.4%** | 368.98 ms |
| minifyContent (async, type=conservative) | 3,911 B | **−61.4%** | 258.35 ms |
| symbols | 2,902 B (−71.3%) | — | 5.96 ms |

## Notes

- TypeScript compiler + Terser pipeline
- Rich type annotations removed by transpiler
