# XML (`.xml`) — Large-File Benchmark

**Source:** 142 lines / 7,256 bytes — Spring XML application context with data source and JPA config

**Agent rating: 7.5/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,256 B | — | — |
| content-view | 4,711 B | **−35.1%** | 0.91 ms |
| applyMinification | 4,042 B | **−44.3%** | 0.89 ms |
| minifyContentSync | 4,042 B | **−44.3%** | 0.87 ms |
| minifyContent (async, type=aggressive) | 4,042 B | **−44.3%** | 0.96 ms |
| symbols | n/a (not supported) | — | — |

## Notes

- HTML/XML comments stripped by aggressive strategy
- Whitespace collapsed between tags
