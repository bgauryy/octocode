# Kotlin (`.kt`) — Large-File Benchmark

**Source:** 206 lines / 7,281 bytes — Android repository: coroutines, Flow, Room, Retrofit

**Agent rating: 7.5/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,281 B | — | — |
| content-view | 4,467 B | **−38.6%** | 13.06 ms |
| applyMinification | 4,467 B | **−38.6%** | 1.6 ms |
| minifyContentSync | 4,467 B | **−38.6%** | 1.82 ms |
| minifyContent (async, type=conservative) | 4,467 B | **−38.6%** | 2.97 ms |
| symbols | 1,795 B (−75.3%) | — | 0.85 ms |

## Notes

- KDoc block comments stripped via c-style remover
- Coroutine annotations preserved
