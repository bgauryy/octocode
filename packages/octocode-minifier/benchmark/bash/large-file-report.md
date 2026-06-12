# Bash (`.bash`) — Large-File Benchmark

**Source:** 235 lines / 7,306 bytes — Blue/green deployment script with full argument documentation

**Agent rating: 7.5/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,306 B | — | — |
| content-view | 4,392 B | **−39.9%** | 1.04 ms |
| applyMinification | 4,392 B | **−39.9%** | 2.92 ms |
| minifyContentSync | 4,392 B | **−39.9%** | 1.31 ms |
| minifyContent (async, type=conservative) | 4,392 B | **−39.9%** | 2.7 ms |
| symbols | 428 B (−94.1%) | — | 0.48 ms |

## Notes

- Hash comments stripped (shebang preserved)
- Shell strategy symbols extraction
