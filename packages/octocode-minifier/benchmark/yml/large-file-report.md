# YAML (`.yml`) — Large-File Benchmark

**Source:** 312 lines / 8,974 bytes — GitHub Actions CI/CD pipeline — multi-job workflow

**Agent rating: 5/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 8,974 B | — | — |
| content-view | 7,008 B | **−21.9%** | 2.66 ms |
| applyMinification | 7,008 B | **−21.9%** | 6.55 ms |
| minifyContentSync | 7,008 B | **−21.9%** | 2.99 ms |
| minifyContent (async, type=conservative) | 7,008 B | **−21.9%** | 5 ms |
| symbols | n/a (not supported) | — | — |

## Notes

- Hash comments stripped
- Low savings expected — real workflows are data-dense
