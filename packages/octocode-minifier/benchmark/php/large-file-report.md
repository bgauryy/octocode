# PHP (`.php`) — Large-File Benchmark

**Source:** 255 lines / 7,159 bytes — Laravel REST controller with PHPDoc (280 lines)

**Agent rating: 9/10 (excellent)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,159 B | — | — |
| content-view | 3,756 B | **−47.5%** | 3.15 ms |
| applyMinification | 3,756 B | **−47.5%** | 3.05 ms |
| minifyContentSync | 3,756 B | **−47.5%** | 2.43 ms |
| minifyContent (async, type=conservative) | 3,756 B | **−47.5%** | 2.72 ms |
| symbols | 1,237 B (−82.7%) | — | 0.49 ms |

## Notes

- PHPDoc block comments stripped by c-style remover
- High doc-comment ratio typical of Laravel source
