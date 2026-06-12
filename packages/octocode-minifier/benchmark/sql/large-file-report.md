# SQL (`.sql`) — Large-File Benchmark

**Source:** 261 lines / 8,970 bytes — E-commerce schema: tables, triggers, stored procedures

**Agent rating: 6/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 8,970 B | — | — |
| content-view | 7,298 B | **−18.6%** | 7.69 ms |
| applyMinification | 7,298 B | **−18.6%** | 3.12 ms |
| minifyContentSync | 7,298 B | **−18.6%** | 6.25 ms |
| minifyContent (async, type=conservative) | 7,298 B | **−18.6%** | 3.02 ms |
| symbols | 5,188 B (−42.2%) | — | 1 ms |

## Notes

- Conservative strategy — SQL line comments stripped
- Block comments in DDL removed
