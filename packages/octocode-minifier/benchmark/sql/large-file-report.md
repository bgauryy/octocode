# SQL (`.sql`) — Large-File Benchmark

**Source:** 261 lines / 8,970 bytes — E-commerce schema: tables, triggers, stored procedures

**Agent rating: 6/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 8,970 B | — | — |
| content-view | 7,298 B | **−18.6%** | 18.13 ms |
| applyMinification | 7,298 B | **−18.6%** | 4.88 ms |
| minifyContentSync | 7,298 B | **−18.6%** | 6.53 ms |
| minifyContent (async, type=conservative) | 7,298 B | **−18.6%** | 3.96 ms |
| symbols | 5,188 B (−42.2%) | — | 9.26 ms |

## Notes

- Conservative strategy — SQL line comments stripped
- Block comments in DDL removed
