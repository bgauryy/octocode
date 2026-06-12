# C (`.c`) — Large-File Benchmark

**Source:** 852 lines / 29,047 bytes — Command-line option parser with URL strings and block comments

**Agent rating: 6/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 29,047 B | — | — |
| content-view | 24,672 B | **−15.1%** | 36.83 ms |
| applyMinification | 24,672 B | **−15.1%** | 23.52 ms |
| minifyContentSync | 24,672 B | **−15.1%** | 10.34 ms |
| minifyContent (async, type=conservative) | 24,672 B | **−15.1%** | 8.92 ms |
| symbols | 4,078 B (−86%) | — | 0.74 ms |

## Notes

- C-style comments stripped conservatively
- Comment-looking URL and string content preserved
