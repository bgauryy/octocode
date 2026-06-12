# GraphQL (`.graphql`) — Large-File Benchmark

**Source:** 249 lines / 6,652 bytes — E-commerce schema: enums, types, queries, mutations

**Agent rating: 5/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 6,652 B | — | — |
| content-view | 5,448 B | **−18.1%** | 0.88 ms |
| applyMinification | 5,448 B | **−18.1%** | 0.84 ms |
| minifyContentSync | 5,448 B | **−18.1%** | 1.09 ms |
| minifyContent (async, type=conservative) | 5,448 B | **−18.1%** | 1.48 ms |
| symbols | n/a (not supported) | — | — |

## Notes

- Description strings and # line comments stripped
- No symbols support — conservative strategy only
