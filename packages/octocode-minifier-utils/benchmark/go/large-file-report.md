# Go (`.go`) — Large-File Benchmark

**Source:** 323 lines / 8,428 bytes — Configurable HTTP client with functional options + retry

**Agent rating: 7.5/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 8,428 B | — | — |
| content-view | 5,518 B | **−34.5%** | 1.91 ms |
| applyMinification | 5,518 B | **−34.5%** | 2.51 ms |
| minifyContentSync | 5,518 B | **−34.5%** | 1.93 ms |
| minifyContent (async, type=conservative) | 5,518 B | **−34.5%** | 3 ms |
| symbols | 1,878 B (−77.7%) | — | 0.4 ms |

## Notes

- Conservative strategy with c-style comment stripping
- GoDoc comments removed
