# JavaScript (`.js`) — Large-File Benchmark

**Source:** 419 lines / 10,668 bytes — Lodash-style utility library with full JSDoc (400 lines)

**Agent rating: 10/10 (excellent)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,668 B | — | — |
| content-view | 4,895 B | **−54.1%** | 6 ms |
| applyMinification | 4,062 B | **−61.9%** | 39.69 ms |
| minifyContentSync | 4,062 B | **−61.9%** | 37.68 ms |
| minifyContent (async, type=terser) | 4,062 B | **−61.9%** | 19.71 ms |
| symbols | 930 B (−91.3%) | — | 10.92 ms |

## Notes

- Terser pipeline: TypeScript transpiler skipped (pure JS), goes direct to terser
- JSDoc block comments stripped by Terser
- Symbols extraction via tsJsStrategy AST parser
