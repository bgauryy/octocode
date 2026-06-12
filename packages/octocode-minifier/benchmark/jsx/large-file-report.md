# JSX (`.jsx`) — Large-File Benchmark

**Source:** 330 lines / 11,996 bytes — React dashboard list with hook state and JSX comment blocks

**Agent rating: 6/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 11,996 B | — | — |
| content-view | 10,554 B | **−12%** | 13.31 ms |
| applyMinification | 9,910 B | **−17.4%** | 56.38 ms |
| minifyContentSync | 9,910 B | **−17.4%** | 61.96 ms |
| minifyContent (async, type=terser) | 9,910 B | **−17.4%** | 92.03 ms |
| symbols | 170 B (−98.6%) | — | 5.26 ms |

## Notes

- TypeScript JSX transform path before Terser
- JSX comments removed while string markers are preserved
