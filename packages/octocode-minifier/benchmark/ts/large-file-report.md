# TypeScript (`.ts`) — Large-File Benchmark

**Source:** 322 lines / 10,126 bytes — Async HTTP client with generics, retry, EventEmitter

**Agent rating: 9.5/10 (excellent)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,126 B | — | — |
| content-view | 7,501 B | **−25.9%** | 7.66 ms |
| applyMinification | 3,911 B | **−61.4%** | 155.86 ms |
| minifyContentSync | 3,911 B | **−61.4%** | 56.35 ms |
| minifyContent (async, type=conservative) | 3,911 B | **−61.4%** | 66.27 ms |
| symbols | 2,902 B (−71.3%) | — | 6.58 ms |

## Notes

- TypeScript compiler + Terser pipeline
- Rich type annotations removed by transpiler
