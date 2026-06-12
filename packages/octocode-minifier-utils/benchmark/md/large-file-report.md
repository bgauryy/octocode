# Markdown (`.md`) — Large-File Benchmark

**Source:** 243 lines / 6,143 bytes — Project README with badges, HTML comments, blockquotes

**Agent rating: 6.5/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 6,143 B | — | — |
| content-view | 4,458 B | **−27.4%** | 8.28 ms |
| applyMinification | 4,458 B | **−27.4%** | 2.5 ms |
| minifyContentSync | 4,458 B | **−27.4%** | 2.19 ms |
| minifyContent (async, type=markdown) | 4,458 B | **−27.4%** | 1.5 ms |
| symbols | n/a (not supported) | — | — |

## Notes

- Badge/shield image lines stripped
- [//]: # pseudo-comments stripped
- HTML block comments stripped while rendered blockquotes are preserved
