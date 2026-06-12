# Shell (`.sh`) — Large-File Benchmark

**Source:** 294 lines / 9,146 bytes — Deployment script: args, SSH, rsync, health-check

**Agent rating: 6/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 9,146 B | — | — |
| content-view | 7,180 B | **−21.5%** | 2.21 ms |
| applyMinification | 7,180 B | **−21.5%** | 25.26 ms |
| minifyContentSync | 7,180 B | **−21.5%** | 1.91 ms |
| minifyContent (async, type=conservative) | 7,180 B | **−21.5%** | 1.72 ms |
| symbols | 745 B (−91.9%) | — | 0.67 ms |

## Notes

- Hash comments stripped but shebang preserved
- Low comment ratio limits savings
