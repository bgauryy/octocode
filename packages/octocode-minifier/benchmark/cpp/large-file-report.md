# C++ (`.cpp`) — Large-File Benchmark

**Source:** 545 lines / 17,663 bytes — Stream report writer with templates and raw string markers

**Agent rating: 4.5/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 17,663 B | — | — |
| content-view | 16,260 B | **−7.9%** | 8.08 ms |
| applyMinification | 16,260 B | **−7.9%** | 5.29 ms |
| minifyContentSync | 16,260 B | **−7.9%** | 9.04 ms |
| minifyContent (async, type=conservative) | 16,260 B | **−7.9%** | 7.27 ms |
| symbols | 5,353 B (−69.7%) | — | 0.28 ms |

## Notes

- C++ line and block comments stripped conservatively
- Raw string and URL markers preserved for scanner edge coverage
