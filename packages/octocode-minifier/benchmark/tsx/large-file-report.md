# TSX (`.tsx`) — Large-File Benchmark

**Source:** 422 lines / 10,509 bytes — App router component with route metadata and Suspense

**Agent rating: 6/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,509 B | — | — |
| content-view | 10,507 B | **−0%** | 28.44 ms |
| applyMinification | 7,518 B | **−28.5%** | 68.54 ms |
| minifyContentSync | 7,518 B | **−28.5%** | 44.81 ms |
| minifyContent (async, type=conservative) | 7,518 B | **−28.5%** | 44.29 ms |
| symbols | 477 B (−95.5%) | — | 13.7 ms |

## Notes

- TypeScript compiler + JSX transform + Terser pipeline
- Large partial-friendly React component tree with type-only declarations
