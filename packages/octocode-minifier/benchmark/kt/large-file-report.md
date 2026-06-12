# Kotlin (`.kt`) — Large-File Benchmark

**Source:** 206 lines / 7,281 bytes — Android repository: coroutines, Flow, Room, Retrofit

**Agent rating: 7.5/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,281 B | — | — |
| content-view | 4,467 B | **−38.6%** | 2.03 ms |
| applyMinification | 4,467 B | **−38.6%** | 1.64 ms |
| minifyContentSync | 4,467 B | **−38.6%** | 1.73 ms |
| minifyContent (async, type=conservative) | 4,467 B | **−38.6%** | 1.94 ms |
| symbols | 1,795 B (−75.3%) | — | 0.24 ms |

## Notes

- KDoc block comments stripped via c-style remover
- Coroutine annotations preserved
