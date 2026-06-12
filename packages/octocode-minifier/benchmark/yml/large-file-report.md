# YAML (`.yml`) — Large-File Benchmark

**Source:** 297 lines / 10,310 bytes — GitHub Actions CI/CD pipeline — multi-job workflow

**Agent rating: 9/10 (excellent)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,310 B | — | — |
| content-view | 3,339 B | **−67.6%** | 1.21 ms |
| applyMinification | 3,339 B | **−67.6%** | 4.72 ms |
| minifyContentSync | 3,339 B | **−67.6%** | 1.16 ms |
| minifyContent (async, type=conservative) | 3,339 B | **−67.6%** | 1.13 ms |
| symbols | n/a (not supported) | — | — |

## Notes

- Hash comments stripped
- Low savings expected — real workflows are data-dense
