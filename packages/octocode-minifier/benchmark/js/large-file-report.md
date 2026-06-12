# JavaScript (`.js`) — Large-File Benchmark

**Source:** 419 lines / 10,670 bytes — Lodash-style utility library with full JSDoc (400 lines)

**Agent rating: 10/10 (excellent)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,670 B | — | — |
| content-view | 4,897 B | **−54.1%** | 3.18 ms |
| applyMinification | 4,064 B | **−61.9%** | 19.72 ms |
| minifyContentSync | 4,064 B | **−61.9%** | 7.92 ms |
| minifyContent (async, type=terser) | 4,064 B | **−61.9%** | 23.73 ms |
| symbols | 930 B (−91.3%) | — | 18.82 ms |

## Notes

- Terser pipeline: TypeScript transpiler skipped (pure JS), goes direct to terser
- JSDoc block comments stripped by Terser
- Symbols extraction via tsJsStrategy AST parser
